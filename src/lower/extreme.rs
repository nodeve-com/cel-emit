//! A `dyn` `math.greatest` or `math.least` as a value: its typed overload where its operands
//! agree, else the number over them ([`super::number`]).

use super::number::wrap;
use super::*;

impl<'a> Lower<'a> {
    /// A `dyn` `math.greatest` or `math.least`: its typed overload where its operands agree, else
    /// the number over them.
    pub(crate) fn extreme(&mut self, e: &'a SpannedExpr) -> Result<Typed, Error> {
        let Some((greatest, args)) = self.extreme_of(e) else {
            return invalid("a math extreme that isn't one");
        };
        let name = if greatest { "greatest" } else { "least" };
        let (items, is_list) = match args {
            [arg] => match &arg.node {
                Expr::List(xs) if xs.iter().any(|x| x.optional) => {
                    return self.extreme_opt(e, xs);
                }
                Expr::List(xs) => {
                    let mut items = Vec::with_capacity(xs.len());
                    for x in xs {
                        items.push(self.number(&x.expr)?);
                    }
                    (items, true)
                }
                // A comprehension folds as it goes: over none, absent.
                Expr::Comprehension(_) => match self.mapped(arg)? {
                    Some(list) => return self.over_fold(greatest, list),
                    None => return unsupported("a math extreme of this comprehension"),
                },
                _ => {
                    let x = self.number(arg)?;
                    let elem = match &x.ty {
                        Type::List(t) => Some((**t).clone()),
                        _ => None,
                    };
                    // A list value, not a literal: its typed overload takes it whole, and an empty
                    // one fails when it runs.
                    if let Some(elem) = elem {
                        return self.typed(e, vec![x], elem);
                    }
                    (vec![x], false)
                }
            },
            _ => {
                let mut items = Vec::with_capacity(args.len());
                for a in args {
                    items.push(self.number(a)?);
                }
                (items, false)
            }
        };
        let Some(first) = items.first().map(|i| i.ty.clone()) else {
            return unsupported(format!("math.{name} of no values"));
        };
        let agree = items.iter().all(|i| i.ty == first);
        if agree && typed_scalar(&first) {
            return if is_list {
                let list = Typed::new(Type::list(first.clone()), Node::List(items));
                self.typed(e, vec![list], first)
            } else {
                self.typed(e, items, first)
            };
        }
        let items = items.into_iter().map(wrap).collect::<Result<Vec<_>, _>>()?;
        if is_list {
            let list = Typed::new(Type::list(Type::Number), Node::List(items));
            return Ok(Typed::call(
                &format!("math_{name}_number_list"),
                vec![list],
                Type::Number,
            ));
        }
        let n = items.len();
        if !(1..=6).contains(&n) {
            return unsupported(format!("math.{name} of {n} values"));
        }
        Ok(Typed::call(
            &format!("math_{name}_number{n}"),
            items,
            Type::Number,
        ))
    }

    /// A `dyn` `math.greatest` or `math.least` of a list literal with optional (`?`) elements:
    /// its typed overload over the elements present, where they agree on an int or a double.
    fn extreme_opt(
        &mut self,
        e: &'a SpannedExpr,
        xs: &'a [cel_core::types::ListElement],
    ) -> Result<Typed, Error> {
        let mut items = Vec::with_capacity(xs.len());
        for x in xs {
            let item = match x.optional {
                true => self.expr(&x.expr)?,
                false => self.number(&x.expr)?,
            };
            let ty = match (&item.ty, x.optional) {
                (Type::Optional(held), true) => (**held).clone(),
                (ty, _) => ty.clone(),
            };
            items.push((item, x.optional, ty));
        }
        let first = items.first().map(|(_, _, ty)| ty.clone());
        let Some(elem) = first.filter(|f| typed_scalar(f) && items.iter().all(|i| i.2 == *f))
        else {
            return unsupported("a math extreme of optional elements of mixed types");
        };
        let made = items.into_iter().map(|(x, opt, _)| (x, opt)).collect();
        let list = Typed::new(Type::list(elem.clone()), Node::ListOpt(made));
        self.typed(e, vec![list], elem)
    }

    /// `math.greatest` or `math.least` of a mapped comprehension: a fold over its int or double
    /// elements, as the checker's `list(int)` or `list(double)` gives them.
    fn over_fold(&self, greatest: bool, list: Box<Typed>) -> Result<Typed, Error> {
        let elem = match &list.ty {
            Type::List(t) => (**t).clone(),
            _ => return invalid("a fold of other than a list"),
        };
        if !typed_scalar(&elem) {
            return unsupported(format!("a math extreme of {}", elem.display()));
        }
        Ok(Typed::new(elem, Node::Extreme(greatest, list)))
    }

    /// `e`'s typed overload on `args`, giving `ty`.
    fn typed(&self, e: &SpannedExpr, args: Vec<Typed>, ty: Type) -> Result<Typed, Error> {
        if !typed_scalar(&ty) {
            return unsupported(format!("a math extreme of {}", ty.display()));
        }
        let id = self.typed_overload(e)?;
        Ok(Typed::call(&id, args, ty))
    }

    /// The checker's overload for `e` that isn't `dyn`'s: the typed one its operands match.
    fn typed_overload(&self, e: &SpannedExpr) -> Result<String, Error> {
        self.info
            .get_reference(e.id)
            .and_then(|r| {
                r.overload_ids
                    .iter()
                    .find(|id| !id.contains("dyn"))
                    .cloned()
            })
            .ok_or_else(|| Error::Unsupported("a math extreme with no typed overload".into()))
    }
}

/// A type a typed overload of `math.greatest` or `math.least` gives.
fn typed_scalar(ty: &Type) -> bool {
    matches!(ty, Type::Int | Type::Uint | Type::Double)
}
