//! `json.decode(text, Message)`: the text decoded once into slots, reading only the keys the message
//! declares ([`Node::Shaped`]), then each field read from its slot on its own, as CEL fills a
//! typed field. Slots number the fields in order from 1, a nested message's before its own
//! fields'; slot 0 is the document itself.
//!
//! A field's read is an expression of its own, so a bad field fails that field alone where the
//! embedder lays the message out field by field, and the message as a whole where it is a value.
//! A required field that is missing fails as a missing key; an optional one is left unset, and
//! a nullable one is null. A nested message reads from its own key the same way, an optional
//! one an optional entry that holds the message where its key is.

use super::*;
use crate::types::Field;

impl<'a> Lower<'a> {
    /// `json.decode(text, Message)`, the message being `e`'s type.
    pub(crate) fn shaped(
        &mut self,
        e: &'a SpannedExpr,
        text: &'a SpannedExpr,
    ) -> Result<Node, Error> {
        let message = self.ty(e)?;
        if !matches!(message, Type::Message(_)) {
            return unsupported("json.decode into other than a message");
        }
        let text = super::dynamic::filled(self.expr(text)?, &Type::String)?;
        let n = self.local_number()?;
        let rest = self.decls.rest.clone().filter(|r| holds(&message, r));
        let slots = Type::List(Box::new(Type::Optional(Box::new(Type::Dyn))));
        let decoded = Typed::new(
            slots.clone(),
            Node::Shaped(Box::new(text), message.clone(), rest),
        );
        let from = Slot {
            slots: &Typed::new(slots, Node::Local(n)),
            of: 0,
        };
        let body = self.read_fields(&message, from, &mut 1)?;
        Ok(Node::Let(n, Box::new(decoded), Box::new(body)))
    }

    pub(crate) fn local_number(&mut self) -> Result<usize, Error> {
        let n = self.next;
        self.next = n
            .checked_add(1)
            .ok_or(Error::Unsupported("too many locals".into()))?;
        Ok(n)
    }

    /// The message `message`, each field read from its slot, the next of `next`, in the message
    /// `from`.
    fn read_fields(
        &mut self,
        message: &Type,
        from: Slot<'_>,
        next: &mut usize,
    ) -> Result<Typed, Error> {
        let mut made = Vec::with_capacity(message.fields().len());
        for (i, field) in message.fields().iter().enumerate() {
            let at = *next;
            *next = at
                .checked_add(1)
                .ok_or(Error::Unsupported("too many fields".into()))?;
            let (value, flagged) = self
                .read_field(field, from, at, next)
                .map_err(|e| e.at(&field.name))?;
            made.push((i, value, flagged));
        }
        Ok(Typed::new(message.clone(), Node::Message(made)))
    }

    /// One field of `from`, in slot `at`, and whether its entry is flagged (`?`): an optional
    /// field a key does not give is left unset.
    fn read_field(
        &mut self,
        field: &Field,
        from: Slot<'_>,
        at: usize,
        next: &mut usize,
    ) -> Result<(Typed, bool), Error> {
        // A field typed an option is one a key may leave out.
        let stripped;
        let field = match &field.ty {
            Type::Optional(held) => {
                stripped = Field {
                    ty: (**held).clone(),
                    required: false,
                    ..field.clone()
                };
                &stripped
            }
            _ => field,
        };
        let get = || from.read("json_field", at, Type::Dyn);
        let (held, nullable) = match &field.ty {
            Type::Nullable(t) => (&**t, true),
            t => (t, false),
        };
        let of = field.ty.cel();
        if matches!(held, Type::Message(_)) {
            if nullable {
                return unsupported("a nullable message in a decode");
            }
            let inner = Slot {
                slots: from.slots,
                of: at,
            };
            if field.required {
                let read = self.read_fields(held, inner, next)?;
                return Ok((read, false));
            }
            // An optional message is a flagged entry, set where its key is.
            let has = from.read("json_has_field", at, Type::Bool);
            let read = self.read_fields(held, inner, next)?;
            let optional = Type::Optional(Box::new(read.ty.clone()));
            let some = Typed::new(optional.clone(), Node::Some(Box::new(read)));
            let none = Typed::new(optional, Node::None);
            return Ok((Typed::cond(has, some, none), true));
        }
        if !matches!(
            held,
            Type::Int
                | Type::Uint
                | Type::Double
                | Type::Bool
                | Type::String
                | Type::Narrow(_)
                | Type::Timestamp
                | Type::Duration
                | Type::Bytes
        ) {
            return unsupported(format!("a {} field in a decode", held.display()));
        }
        let read = |source: Typed, to: Type| {
            let read = Typed::new(to.clone(), Node::FromDyn(Box::new(source), to));
            if field.ty.narrow() {
                super::message::within(read, &field.ty)
            } else {
                Ok(read)
            }
        };
        let maybe = Type::Optional(Box::new(Type::Dyn));
        let found = || from.read("json_optional_field", at, maybe.clone());
        if nullable && field.ty.narrow() {
            return unsupported("a nullable narrow field in a decode");
        }
        let (value, flagged) = match (nullable, field.required) {
            (false, true) => (read(get(), of.clone())?, false),
            (true, true) => {
                let some = Typed::new(maybe.clone(), Node::Some(Box::new(get())));
                (read(some, of.clone())?, false)
            }
            (true, false) => (read(found(), of.clone())?, false),
            (false, false) => {
                if field.ty.narrow() {
                    return unsupported("an optional narrow field in a decode");
                }
                (read(found(), Type::Optional(Box::new(of.clone())))?, true)
            }
        };
        Ok((value, flagged))
    }
}

/// The message in slot `of` of a decode's `slots`.
#[derive(Clone, Copy)]
struct Slot<'t> {
    slots: &'t Typed,
    of: usize,
}

impl Slot<'_> {
    /// `f` of the field in slot `at`, typed `ty`.
    fn read(self, f: &str, at: usize, ty: Type) -> Typed {
        let n = |i: usize| Typed::int(i64::try_from(i).unwrap_or(i64::MAX));
        Typed::call(f, vec![self.slots.clone(), n(self.of), n(at)], ty)
    }
}

/// Whether `ty`, or a message inside it by fields alone, has a field named `name`.
fn holds(ty: &Type, name: &str) -> bool {
    match ty {
        Type::Nullable(t) => holds(t, name),
        Type::Message(fields) => fields.iter().any(|f| f.name == name || holds(&f.ty, name)),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use crate::types::{Field, Type, Width};
    use crate::{lower, Decls};

    fn decls() -> Decls {
        let mut d = Decls::default();
        let messages = [
            ("M", "n", Type::Int),
            ("L", "l", Type::list(Type::Int)),
            ("W", "w", Type::Narrow(Width::I32)),
        ];
        for (name, field, ty) in messages {
            let field = Field {
                name: field.into(),
                ty,
                required: false,
            };
            d.messages.insert(name.into(), Type::Message(vec![field]));
        }
        d
    }

    #[test]
    fn a_message_name_is_a_type_argument() {
        let t = lower("json.decode('{}', M)", &decls()).unwrap();
        assert!(matches!(t.ty, Type::Message(_)));
    }

    #[test]
    fn a_message_with_the_rest_field_keeps_the_other_keys_under_its_name() {
        let rest = |message: &str, name: &str| {
            let mut d = decls();
            d.rest = Some(name.into());
            let t = lower(&format!("json.decode('{{}}', {message})"), &d).unwrap();
            let mut found = None;
            crate::walk::nodes(&t, &mut |n| {
                if let crate::Node::Shaped(_, _, rest) = &n.node {
                    found = Some(rest.clone());
                }
            });
            found.unwrap()
        };
        assert_eq!(rest("M", "n"), Some("n".to_string()));
        assert_eq!(rest("M", "other"), None);
    }

    #[test]
    fn an_optional_message_is_an_optional_entry_where_its_key_is() {
        let inner = Type::Message(vec![Field {
            name: "k".into(),
            ty: Type::Int,
            required: true,
        }]);
        let outer = |required| {
            Type::Message(vec![Field {
                name: "n".into(),
                ty: inner.clone(),
                required,
            }])
        };
        let mut d = decls();
        d.messages.insert("P".into(), outer(false));
        let top = crate::split(lower("json.decode('{}', P)", &d).unwrap());
        let (_, n) = top.fields.into_iter().next().unwrap();
        assert_eq!(n.ty, Type::Optional(Box::new(inner.clone())));
        d.messages.insert("P".into(), outer(true));
        let top = crate::split(lower("json.decode('{}', P)", &d).unwrap());
        let (_, n) = top.fields.into_iter().next().unwrap();
        let (_, k) = crate::split(n).fields.into_iter().next().unwrap();
        assert_eq!(k.ty, Type::Int);
    }

    #[test]
    fn what_is_no_message_or_has_no_read_is_refused() {
        for (source, why) in [
            ("json.decode('{}', int)", "other than a message"),
            ("json.decode('{}', L)", "list"),
            ("json.decode('{}', W)", "optional narrow"),
        ] {
            let err = lower(source, &decls()).unwrap_err();
            assert!(
                err.to_string().starts_with("unsupported") && err.to_string().contains(why),
                "{source}: {err}"
            );
        }
    }
}
