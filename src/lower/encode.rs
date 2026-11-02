//! `json.encode(x)`: `x` written as JSON in the module. A message's fields, a message name's, a
//! literal's, or a map literal's, are written as an object member by member as each computes
//! ([`Node::Encode`]); a field whose variables hold no value is left out, and so is a nested
//! object left with none. With [`Decls::skip_failed`], so is a field whose value errors or holds a
//! NaN or an infinity, which JSON can't hold. Any other value is written whole
//! ([`Node::Written`]).

use super::dynamic::fits;
use super::parts::{held_by_field, is_fields};
use super::scope::wrap;
use super::*;
use crate::tree::{split, Member, Object, Out};
use crate::walk;

impl<'a> Lower<'a> {
    /// `json.encode(arg)`: an object written as it computes, or the call on `arg` as a dyn.
    pub(crate) fn encoded(&mut self, arg: &'a SpannedExpr) -> Result<Node, Error> {
        let v = self.value(arg)?;
        if is_object(&v) {
            return Ok(Node::Encode(self.object(v)?));
        }
        Ok(Node::Call("json_encode".into(), vec![self.written(v)?]))
    }

    /// `v`, which [`is_object`], as the object its fields write.
    fn object(&mut self, v: Typed) -> Result<Object, Error> {
        let s = split(v);
        let mut members = Vec::new();
        for (key, v) in s.fields {
            let v = held_by_field(v);
            if is_fields(&v) {
                let out = Out::Object(self.object(v)?);
                members.push(Member {
                    key,
                    present: None,
                    out,
                });
                continue;
            }
            // A message is there where its required reads are, else where any flagged one is.
            let has = |vars: Vec<usize>, join: fn(Typed, Typed) -> Typed| {
                let mut vars = vars;
                vars.sort_unstable();
                vars.dedup();
                let tests = vars
                    .into_iter()
                    .map(|v| Typed::new(Type::Bool, Node::Has(v)));
                tests.reduce(join)
            };
            let read = reading(&v, &s.lets);
            let present = has(walk::vars(&wrap(unflagged(&v), &read)), Typed::and)
                .or_else(|| has(walk::vars(&wrap(v.clone(), &read)), Typed::or));
            let kept = self.decls.skip_failed && !is_literal(&v);
            let rest = self.decls.rest.as_deref() == Some(key.as_str());
            let out = match v.ty.clone() {
                _ if is_object(&v) => Out::Object(self.object(v)?),
                Type::String if rest => Out::Rest(v, kept),
                Type::Optional(held) if *held == Type::String && rest => Out::Rest(v, kept),
                Type::Optional(held) => {
                    let local = self.local_number()?;
                    let value = self.written(Typed::new(*held, Node::Local(local)))?;
                    Out::Optional {
                        optional: v,
                        local,
                        value,
                        kept,
                    }
                }
                _ => Out::Value(self.written(v)?, kept),
            };
            members.push(Member { key, present, out });
        }
        Ok(Object {
            lets: s.lets,
            members,
        })
    }

    /// `t` as a dyn: a float32 or a list of them as stored, so each is written in its own digits.
    fn written(&self, t: Typed) -> Result<Typed, Error> {
        let t = match t {
            Typed {
                node: Node::Widen(stored),
                ..
            } => *stored,
            t => t,
        };
        if t.ty == Type::Dyn {
            return Ok(t);
        }
        if !t.ty.writable() {
            return unsupported(format!("json.encode of a {}", t.ty.display()));
        }
        if t.ty.dyn_into(&Type::Dyn) {
            return fits(t, &Type::Dyn);
        }
        Ok(Typed::new(
            Type::Dyn,
            Node::Written(Box::new(t), self.decls.rest.clone()),
        ))
    }
}

/// Whether `t` is a message literal, `?` entries and all, or [`is_fields`]: an object written
/// member by member.
fn is_object(t: &Typed) -> bool {
    is_fields(t) || matches!((&t.ty, &t.node), (Type::Message(_), Node::Message(_)))
}

/// Those of `lets` that `v` reads, directly or through another of them, in order.
fn reading(v: &Typed, lets: &[(usize, Typed)]) -> Vec<(usize, Typed)> {
    let mut read = walk::locals(v);
    let mut kept: Vec<_> = lets
        .iter()
        .rev()
        .filter(|(n, value)| {
            let needed = read.contains(n);
            if needed {
                read.extend(walk::locals(value));
            }
            needed
        })
        .cloned()
        .collect();
    kept.reverse();
    kept
}

/// Whether `t` is a constant, which computes without error.
fn is_literal(t: &Typed) -> bool {
    matches!(
        t.node,
        Node::Int(_) | Node::Uint(_) | Node::Double(_) | Node::Bool(_) | Node::Str(_) | Node::Null
    )
}

/// `v`, a message literal without its flagged entries, which are set only where they hold: the
/// message is there without them.
fn unflagged(v: &Typed) -> Typed {
    match &v.node {
        Node::Message(entries) => Typed::new(
            v.ty.clone(),
            Node::Message(entries.iter().filter(|(_, _, f)| !f).cloned().collect()),
        ),
        _ => v.clone(),
    }
}

#[cfg(test)]
mod tests {
    use crate::tree::{Object, Out};
    use crate::{lower, Decls, Node};

    fn skips(source: &str, skip_failed: bool) -> usize {
        fn flagged(o: &Object) -> usize {
            o.members
                .iter()
                .map(|m| match &m.out {
                    Out::Value(_, kept) | Out::Rest(_, kept) => usize::from(*kept),
                    Out::Optional { kept, .. } => usize::from(*kept),
                    Out::Object(o) => flagged(o),
                })
                .sum()
        }
        let decls = Decls {
            skip_failed,
            ..Decls::default()
        };
        match lower(source, &decls).unwrap().node {
            Node::Encode(o) => flagged(&o),
            _ => 0,
        }
    }

    #[test]
    fn a_field_that_can_fail_is_left_out_only_where_asked_and_a_constant_never_is() {
        let source = "json.encode({'a': 1 / 0, 'o': {'b': 2.0 / 0.0}, 'c': 3})";
        assert_eq!(skips(source, false), 0);
        assert_eq!(skips(source, true), 2);
    }

    #[test]
    fn a_bind_is_one_let_of_the_object_however_many_members_read_it() {
        let source = "json.encode(cel.bind(x, [1].map(i, i), {'a': x, 'b': x}))";
        let Node::Encode(object) = lower(source, &Decls::default()).unwrap().node else {
            panic!("no object");
        };
        assert_eq!(object.lets.len(), 1);
        for member in &object.members {
            let Out::Value(value, _) = &member.out else {
                panic!("no value");
            };
            crate::walk::nodes(value, &mut |n| assert!(!matches!(n.node, Node::Let(..))));
        }
    }

    #[test]
    fn a_value_written_whole_is_not_skipped() {
        assert_eq!(skips("json.encode(1.0 / 0.0)", true), 0);
    }
}
