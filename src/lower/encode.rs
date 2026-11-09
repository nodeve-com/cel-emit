//! `json.encode(x)`: `x` written as JSON in the module. A message's fields, a message name's, a
//! literal's, or a map literal's, are written as an object member by member as each computes
//! ([`Node::Encode`]), and so is each item of a `map` whose body is such a literal
//! ([`Node::EncodeEach`]). A member written from an optional is left out where it holds none. With
//! [`Decls::skip_failed`], a field whose value errors or holds a NaN or an infinity, which JSON
//! can't hold, is left out too. Any other value is written whole
//! ([`Node::Written`]).

use super::dynamic::fits;
use super::fold::{fold_of, Fold};
use super::parts::{held_by_field, is_fields};
use super::*;
use crate::tree::{split, Member, Object, Out};

impl<'a> Lower<'a> {
    /// `json.encode(arg)`: an object written as it computes, or the call on `arg` as a dyn.
    pub(crate) fn encoded(&mut self, arg: &'a SpannedExpr) -> Result<Node, Error> {
        if let Some(each) = self.encode_each(arg)? {
            return Ok(each);
        }
        let v = self.value(arg)?;
        if is_object(&v) {
            return Ok(Node::Encode(self.object(v)?));
        }
        Ok(Node::Call("json_encode".into(), vec![self.written(v)?]))
    }

    /// `arg`, a `map` or filtered `map` whose body is a map literal of constant string keys, as
    /// an array of objects written as they compute; none for any other expression.
    fn encode_each(&mut self, arg: &'a SpannedExpr) -> Result<Option<Node>, Error> {
        let Expr::Comprehension(c) = &arg.node else {
            return Ok(None);
        };
        let Ok((Fold::Map(_), body)) = fold_of(c) else {
            return Ok(None);
        };
        if self.empty_loop(arg) || !constant_keys(body) {
            return Ok(None);
        }
        let (list, pred, body) = match self.fold(c, true)? {
            Node::Map(list, body) => (list, None, body),
            Node::Keep(list, pred, body) => (list, Some(pred), body),
            _ => return invalid("a map that is no map"),
        };
        let body = self.share(*body)?;
        let object = Typed::new(Type::String, Node::Encode(self.object(body)?));
        Ok(Some(Node::EncodeEach(list, pred, Box::new(object))))
    }

    /// `v`, which [`is_object`], as the object its fields write.
    fn object(&mut self, v: Typed) -> Result<Object, Error> {
        let s = split(v);
        let mut members = Vec::new();
        for (key, v) in s.fields {
            members.push(self.member(key, held_by_field(v))?);
        }
        Ok(Object {
            lets: s.lets,
            members,
        })
    }

    /// The member `key` of an object, written from `v`.
    fn member(&mut self, key: String, v: Typed) -> Result<Member, Error> {
        if is_fields(&v) {
            let out = Out::Object(self.object(v)?);
            return Ok(Member { key, out });
        }
        let kept = self.decls.skip_failed && !is_literal(&v);
        let out = match v.ty.clone() {
            _ if is_object(&v) => Out::Object(self.object(v)?),
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
        Ok(Member { key, out })
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
        Ok(Typed::new(Type::Dyn, Node::Written(Box::new(t))))
    }
}

/// Whether `e` is a map literal whose keys are all strings, none optional.
fn constant_keys(e: &SpannedExpr) -> bool {
    match &e.node {
        Expr::Map(entries) => entries
            .iter()
            .all(|m| !m.optional && matches!(m.key.node, Expr::String(_))),
        _ => false,
    }
}

/// Whether `t` is a message literal, `?` entries and all, or [`is_fields`]: an object written
/// member by member.
fn is_object(t: &Typed) -> bool {
    is_fields(t) || matches!((&t.ty, &t.node), (Type::Message(_), Node::Message(_)))
}

/// Whether `t` is a constant, which computes without error.
fn is_literal(t: &Typed) -> bool {
    matches!(
        t.node,
        Node::Int(_) | Node::Uint(_) | Node::Double(_) | Node::Bool(_) | Node::Str(_) | Node::Null
    )
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
                    Out::Value(_, kept) => usize::from(*kept),
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
