//! A struct literal as a value ([`Node::Message`]).

use cel_core::types::StructField;

use super::*;
use crate::types::Field;

/// `v` held to a field's stored `ty`, where it holds a width. A nullable field's null needs no
/// check, nor does a read of that very type, which holds its width already.
pub(super) fn within(t: Typed, ty: &Type) -> Result<Typed, Error> {
    if !ty.narrow() || t.node == Node::None {
        return Ok(t);
    }
    // A value that is no null fills an optional field set, held as the whole optional.
    if let Type::Optional(inner) = ty {
        if t.ty == inner.cel() {
            let some = Typed::new(ty.cel(), Node::Some(Box::new(t)));
            return Ok(Typed::new(
                ty.cel(),
                Node::Within(Box::new(some), ty.clone()),
            ));
        }
    }
    // A value that is no null fills a nullable field as its inner type.
    let stored = match ty {
        Type::Nullable(inner) if t.ty == inner.cel() => inner,
        ty => ty,
    };
    if t.ty != stored.cel() {
        return unsupported("a value of other than the field's type into a narrow field");
    }
    if t.read_as(stored).is_some() {
        return Ok(t);
    }
    Ok(Typed::new(
        t.ty.clone(),
        Node::Within(Box::new(t), stored.clone()),
    ))
}

/// `t`, a value filling a field of `ty`, held as the field holds it: a nullable or optional
/// field holds a value that is no null as `Some`.
pub(super) fn held(t: Typed, ty: &Type) -> Typed {
    match ty {
        Type::Nullable(_) | Type::Optional(_) if t.node != Node::None && t.ty != ty.cel() => {
            Typed::new(ty.cel(), Node::Some(Box::new(t)))
        }
        _ => t,
    }
}

/// `t` held as a value: its message type needs a layout.
pub(super) fn whole(t: Typed) -> Result<Typed, Error> {
    if t.ty.laid_out() {
        return Ok(t);
    }
    unsupported("a message holding an absent or null value, as a value")
}

impl<'a> Lower<'a> {
    /// A struct literal as a value of its message type: each entry typed as its field.
    pub(crate) fn message(&mut self, e: &'a SpannedExpr) -> Result<Typed, Error> {
        let Expr::Struct { type_name, fields } = &e.node else {
            return invalid("a message where a value is wanted");
        };
        let Some(name) = self.path(type_name) else {
            return unsupported("a qualified message name");
        };
        let ty = self
            .decls
            .messages
            .get(&name)
            .cloned()
            .ok_or_else(|| Error::Invalid(format!("no message {name} to build")))?;
        let mut made = Vec::with_capacity(fields.len());
        for f in fields {
            made.push(self.set_field(&name, &ty, f)?);
        }
        for field in ty.fields() {
            let set = fields.iter().any(|f| f.name == field.name && !f.optional);
            if field.required && !set {
                return invalid(format!("{name}{{}} leaves out {}", field.name));
            }
        }
        Ok(Typed::new(ty, Node::Message(made)))
    }

    /// The entry `f` of message `name` of type `ty`: its field's place, value, and whether it is
    /// flagged.
    fn set_field(
        &mut self,
        name: &str,
        ty: &Type,
        f: &'a StructField,
    ) -> Result<(usize, Typed, bool), Error> {
        let i = ty
            .fields()
            .iter()
            .position(|x| x.name == f.name)
            .ok_or_else(|| Error::Invalid(format!("{name} has no field {}", f.name)))?;
        let field = ty
            .fields()
            .get(i)
            .ok_or_else(|| Error::Invalid(f.name.clone()))?;
        let declared = self.decls.tags.get(&format!("{name}.{}", f.name));
        if let (Some(declared), Some((given, mismatch))) = (declared, self.tagged(&f.value)) {
            if *declared != given {
                return invalid(mismatch(&f.name, declared, &given));
            }
        }
        let v = self.entry(f, field).map_err(|e| e.at(&f.name))?;
        Ok((i, v, f.optional))
    }

    /// A struct literal as a value that is emitted whole.
    pub(crate) fn message_value(&mut self, e: &'a SpannedExpr) -> Result<Typed, Error> {
        whole(self.message(e)?)
    }

    /// An entry's value, typed as the field it fills: a `?` entry is an optional of it.
    fn entry(&mut self, f: &'a StructField, field: &Field) -> Result<Typed, Error> {
        let v = self.expr(&f.value)?;
        if f.optional {
            let (of, optional) = match &field.ty {
                Type::Optional(t) if matches!(**t, Type::Nullable(_)) => {
                    return unsupported("a ?-entry into an optional of a nullable");
                }
                Type::Optional(t) => (t.cel(), true),
                Type::Nullable(t) => (t.cel(), false),
                t => (t.cel(), false),
            };
            return match &v.ty {
                _ if field.ty.narrow() && !optional => unsupported("a ?-entry into a narrow field"),
                Type::Optional(held) if **held == of && optional => within(v, &field.ty),
                Type::Optional(held) if **held == of => Ok(v),
                _ => invalid("a ?-entry that is no optional of its field's type"),
            };
        }
        let v = super::nullable::fit(v, &field.ty.cel())?;
        Ok(held(within(v, &field.ty)?, &field.ty))
    }

    /// `e`, when it names a bound message, as a value.
    pub(crate) fn bound_value(&self, e: &SpannedExpr) -> Result<Option<Typed>, Error> {
        let Expr::Ident(name) = &e.node else {
            return Ok(None);
        };
        match self.bound(name).cloned() {
            Some(v) if super::parts::is_message(&v) => whole(v).map(Some),
            other => Ok(other),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::types::{Field, Type, Width};
    use crate::{lower, Decls, Node};

    fn field_of(name: &str, ty: Type) -> Field {
        Field {
            name: name.into(),
            ty,
            required: false,
        }
    }

    fn decls() -> Decls {
        let field = field_of;
        let mut d = Decls::default();
        let narrow = vec![field("n", Type::Int), field("w", Type::Narrow(Width::I32))];
        d.messages.insert("Narrow".into(), Type::Message(narrow));
        let plain = vec![field("n", Type::Int)];
        d.messages.insert("M".into(), Type::Message(plain));
        let absent = vec![field("l", Type::list(Type::Nullable(Box::new(Type::Int))))];
        d.messages.insert("Absent".into(), Type::Message(absent));
        d
    }

    /// `d` with `tag(x, 'a')`, which reads as its first argument and tags it `a`.
    fn tagging(mut d: Decls) -> Decls {
        d.expansions.push(crate::Expansion {
            decl: crate::Decl::new("tag").with_overload(
                "tag_double",
                vec![Type::Double, Type::String],
                Type::Double,
            ),
            expand: |args, tags| {
                let [x, _] = args else {
                    return Err(crate::Error::Invalid("two arguments".into()));
                };
                let path = x.path.clone().unwrap_or_default();
                match tags.contains_key(&path) {
                    true => Ok(x.value.clone()),
                    false => Err(crate::Error::Invalid(format!("{path} has no tag"))),
                }
            },
            mismatch: |field, declared, given| format!("{field}: {declared}, not {given}"),
        });
        let x = crate::Name::var(Type::Double, 0);
        d.names.insert("x".into(), x);
        d.tags.insert("x".into(), "a".into());
        let tagged = vec![Field {
            name: "t".into(),
            ty: Type::Double,
            required: true,
        }];
        d.messages.insert("Tagged".into(), Type::Message(tagged));
        d.tags.insert("Tagged.t".into(), "b".into());
        d
    }

    #[test]
    fn an_expansion_reads_a_names_tag_and_becomes_what_it_gives() {
        let tree = lower("tag(x, 'c')", &tagging(decls())).unwrap();
        assert!(matches!(tree.node, Node::Var(0)), "{tree:?}");
    }

    #[test]
    fn a_field_refuses_a_value_tagged_otherwise_in_the_expansions_words() {
        let d = tagging(decls());
        let refused = lower("Tagged{t: tag(x, 'c')}", &d).unwrap_err();
        assert!(refused.to_string().contains("t: b, not c"), "{refused}");
        lower("Tagged{t: tag(x, 'b')}", &d).unwrap();
    }

    #[test]
    fn a_literal_is_a_message_value() {
        let t = lower("M{n: 1} == M{}", &decls()).unwrap();
        let Node::Call(id, operands) = t.node else {
            panic!("{t:?}");
        };
        assert_eq!(id, "equals");
        assert!(operands.iter().all(|o| matches!(o.node, Node::Message(_))));
    }

    #[test]
    fn what_has_no_equality_is_refused_when_lowered() {
        for source in [
            "Narrow{n: 1} == Narrow{}",
            "optional.ofNonZeroValue(M{n: 1}).hasValue()",
        ] {
            let err = lower(source, &decls()).unwrap_err();
            assert!(
                err.to_string().starts_with("unsupported"),
                "{source}: {err}"
            );
        }
    }

    #[test]
    fn a_read_of_the_fields_own_type_is_not_held_to_its_width_again() {
        let list = Type::list(Type::Narrow(Width::I32));
        let mut d = decls();
        let fields = vec![
            field_of("l", list.clone()),
            field_of("o", Type::list(Type::Narrow(Width::I16))),
        ];
        d.messages.insert("L".into(), Type::Message(fields));
        d.names.insert("x".into(), crate::Name::var(list, 0));
        let entry = |source: &str| {
            let Node::Message(made) = lower(source, &d).unwrap().node else {
                panic!("{source}");
            };
            made.into_iter().next().unwrap().1.node
        };
        assert!(matches!(entry("L{l: x}"), Node::Widen(_)));
        assert!(matches!(entry("L{o: x}"), Node::Within(..)));
    }

    #[test]
    fn a_message_with_no_layout_is_refused_as_a_value() {
        let err = lower("optional.of(Absent{}).hasValue()", &decls()).unwrap_err();
        assert!(err.to_string().starts_with("unsupported"), "{err}");
    }

    #[test]
    fn an_optional_of_a_nullable_is_refused() {
        let mut d = decls();
        let nested = Type::Optional(Box::new(Type::Nullable(Box::new(Type::Int))));
        d.messages
            .insert("Deep".into(), Type::Message(vec![field_of("o", nested)]));
        for source in ["Deep{?o: optional.of(1)}.o", "Deep{o: 1}.o"] {
            assert!(lower(source, &d).is_err(), "{source}");
        }
    }
}
