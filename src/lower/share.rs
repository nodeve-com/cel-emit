//! A subtree repeated where it always runs is computed once: bound in a local at the top of the
//! region that holds it, and read from there.
//!
//! A region is what runs together: a whole expression, a branch, a side of `&&` or `||`, a
//! comprehension's body. Within one, a subtree that fails fails the region either way, so binding
//! it first changes no result. A subtree never leaves its region: outside it, one could run that
//! never ran, and fail where the expression gave a value. Only subtrees of one region are
//! compared, so a loop's variable read in two means the same variable.

use std::cmp::Reverse;

use super::scope::wrap;
use super::*;
use crate::walk::{leaf, thin};

impl Lower<'_> {
    /// `t`, a region, with each repeated subtree in a local, and so each region under it.
    pub(crate) fn share(&mut self, mut t: Typed) -> Result<Typed, Error> {
        let mut lets: Vec<(usize, Typed)> = Vec::new();
        loop {
            let values = lets.iter_mut().map(|(_, value)| value);
            let mut roots: Vec<&mut Typed> = std::iter::once(&mut t).chain(values).collect();
            let Some(x) = repeated(&mut roots) else {
                break;
            };
            let n = self.local_number()?;
            let local = Typed::new(x.ty.clone(), Node::Local(n));
            for root in &mut roots {
                replace(root, &x, &local);
            }
            lets.push((n, x));
        }
        // The largest bind first, so each value reads only binds found after it.
        lets.reverse();
        for (_, value) in &mut lets {
            self.share_under(value)?;
        }
        self.share_under(&mut t)?;
        Ok(wrap(t, &lets))
    }

    /// Each region under `t` shared.
    fn share_under(&mut self, t: &mut Typed) -> Result<(), Error> {
        let (runs, regions) = parts(t);
        for x in runs {
            self.share_under(x)?;
        }
        for x in regions {
            let region = std::mem::replace(x, Typed::bool(false));
            *x = self.share(region)?;
        }
        Ok(())
    }
}

/// The largest subtree, worth a local, found twice among what always runs of `roots`.
fn repeated(roots: &mut [&mut Typed]) -> Option<Typed> {
    let mut seen: Vec<(usize, Typed)> = Vec::new();
    for root in roots.iter_mut() {
        always(root, &mut |x| {
            if !trivial(x) {
                seen.push((size(x), x.clone()));
            }
        });
    }
    seen.sort_by_key(|(n, _)| Reverse(*n));
    let twice = |(i, (n, x)): &(usize, &(usize, Typed))| {
        let others = seen.iter().skip(i.saturating_add(1));
        let same_size = others.take_while(|(m, _)| m == n);
        same_size.into_iter().any(|(_, y)| y == x)
    };
    let found = seen.iter().enumerate().find(twice);
    found.map(|(_, (_, x))| x.clone())
}

/// `f` on `t` and each node under it that runs whenever `t` does.
fn always(t: &mut Typed, f: &mut impl FnMut(&mut Typed)) {
    f(t);
    for x in parts(t).0 {
        always(x, f);
    }
}

/// `t`'s children: those that run whenever it does, and the regions, which might not run, or
/// whose error `t` might absorb. [`Node::Encode`] leaves a failing member out, so all of it stays
/// as written.
fn parts(t: &mut Typed) -> (Vec<&mut Typed>, Vec<&mut Typed>) {
    match &mut t.node {
        thin!(x) | Node::Shaped(x, ..) | Node::Written(x, _) | Node::Matches(x, ..) => {
            (vec![&mut **x], Vec::new())
        }
        // A fold's comprehension stays under it: only its parts can leave.
        Node::Extreme(_, x) | Node::Sum(x) | Node::Count(x) => parts(x),
        Node::Call(id, args) => call_parts(id, args),
        Node::List(items) => (items.iter_mut().collect(), Vec::new()),
        Node::ListOpt(items) => (items.iter_mut().map(|(x, _)| x).collect(), Vec::new()),
        Node::Message(entries) => (entries.iter_mut().map(|(_, x, _)| x).collect(), Vec::new()),
        Node::MapOf(entries) => (
            entries.iter_mut().flat_map(|(k, v)| [k, v]).collect(),
            Vec::new(),
        ),
        Node::MapOpt(entries) => (
            entries.iter_mut().flat_map(|(k, v, _)| [k, v]).collect(),
            Vec::new(),
        ),
        Node::And(a, b) | Node::Or(a, b) => (Vec::new(), vec![&mut **a, &mut **b]),
        Node::Cond(c, a, b) | Node::Keep(c, a, b) => (vec![&mut **c], vec![&mut **a, &mut **b]),
        Node::EncodeEach(list, pred, body) => (
            vec![&mut **list],
            pred.iter_mut()
                .map(|p| &mut **p)
                .chain([&mut **body])
                .collect(),
        ),
        Node::Map(x, body)
        | Node::All(_, x, body)
        | Node::First(x, body)
        | Node::Let(_, x, body) => (vec![&mut **x], vec![&mut **body]),
        _ => (Vec::new(), Vec::new()),
    }
}

/// A call's arguments: those that run whenever it does, and the regions. CEL reads an optional's
/// fallback only where it holds nothing.
fn call_parts<'t>(id: &str, args: &'t mut [Typed]) -> (Vec<&'t mut Typed>, Vec<&'t mut Typed>) {
    let fallback = matches!(id, "optional_or_value" | "optional_or_optional");
    let runs = if fallback {
        args.len().min(1)
    } else {
        args.len()
    };
    let (runs, regions) = args.split_at_mut(runs);
    (runs.iter_mut().collect(), regions.iter_mut().collect())
}

/// Whether `t` costs no more to read again than a local does.
fn trivial(t: &Typed) -> bool {
    match &t.node {
        thin!(x) => trivial(x),
        leaf!() => true,
        _ => false,
    }
}

fn size(t: &Typed) -> usize {
    let mut n = 0usize;
    crate::walk::nodes(t, &mut |_| n = n.saturating_add(1));
    n
}

/// Every always-run subtree of `root` equal to `x`, replaced by `local`.
fn replace(root: &mut Typed, x: &Typed, local: &Typed) {
    always(root, &mut |y| {
        if *y == *x {
            *y = local.clone();
        }
    });
}

#[cfg(test)]
mod tests {
    use crate::types::Type;
    use crate::{lower, walk, Decls, Name};

    /// How many locals `cel`, over a dyn `m`, binds.
    fn binds(cel: &str) -> usize {
        let mut decls = Decls::default();
        decls.names.insert("m".into(), Name::var(Type::Dyn, 0));
        decls.names.insert("s".into(), Name::var(Type::String, 1));
        let tree = lower(cel, &decls).unwrap();
        let mut n = 0usize;
        walk::nodes(&tree, &mut |t| {
            if matches!(t.node, crate::Node::Let(..)) {
                n = n.saturating_add(1);
            }
        });
        n
    }

    #[test]
    fn a_repeated_subtree_binds_once_with_what_repeats_inside_it() {
        assert_eq!(binds("[s + 'a', s + 'a']"), 1);
        assert_eq!(
            binds("[size(s + 'a'), size(s + 'a'), size(s + 'a' + 'b')]"),
            2
        );
        assert_eq!(binds("[s, s, m, m, 1, 1]"), 0);
    }

    #[test]
    fn a_subtree_stays_in_the_region_holding_it() {
        assert_eq!(binds("s == '' ? s + 'a' : s + 'a'"), 0);
        assert_eq!(binds("has(m.a) ? (m.a.b + 't') + (m.a.b + 't') : ''"), 1);
        assert_eq!(binds("s + 'a' == '' && s + 'a' == 'b'"), 0);
        assert_eq!(binds("[1].map(x, s + 'a').size() + size(s + 'a')"), 0);
    }

    #[test]
    fn loops_at_one_depth_share_nothing_through_their_variable() {
        assert_eq!(binds("[[1].map(x, x + 1), [2].map(y, y + 1)]"), 0);
        assert_eq!(binds("[1].map(x, [x + 1, x + 1])"), 1);
    }
}
