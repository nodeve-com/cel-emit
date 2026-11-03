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
        let mut slots = Slots {
            typed: Typed::new(slots, Node::Local(n)),
            next: 1,
        };
        let body = self.read_fields(&message, 0, &mut slots)?;
        Ok(Node::Let(n, Box::new(decoded), Box::new(body)))
    }

    pub(crate) fn local_number(&mut self) -> Result<usize, Error> {
        let n = self.next;
        self.next = n
            .checked_add(1)
            .ok_or(Error::Unsupported("too many locals".into()))?;
        Ok(n)
    }

    /// The message `message`, in slot `of`, each field read from its slot, the next of `slots`.
    fn read_fields(
        &mut self,
        message: &Type,
        of: usize,
        slots: &mut Slots,
    ) -> Result<Typed, Error> {
        let mut made = Vec::with_capacity(message.fields().len());
        for (i, field) in message.fields().iter().enumerate() {
            let at = slots.next;
            slots.next = at
                .checked_add(1)
                .ok_or(Error::Unsupported("too many fields".into()))?;
            let (value, flagged) = self
                .read_field(field, Place { of, at }, slots)
                .map_err(|e| e.at(&field.name))?;
            made.push((i, value, flagged));
        }
        Ok(Typed::new(message.clone(), Node::Message(made)))
    }

    /// One field, in `place`, and whether its entry is flagged (`?`): an optional field a key
    /// does not give is left unset.
    fn read_field(
        &mut self,
        field: &Field,
        place: Place,
        slots: &mut Slots,
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
        let (held, nullable) = match &field.ty {
            Type::Nullable(t) => (&**t, true),
            t => (t, false),
        };
        if !matches!(held, Type::Message(_)) {
            return scalar_field(field, nullable, place, slots);
        }
        if nullable {
            return unsupported("a nullable message in a decode");
        }
        let inner = Place {
            of: place.at,
            ..place
        };
        if field.required {
            return Ok((self.read_fields(held, inner.of, slots)?, false));
        }
        // An optional message is a flagged entry, set where its key is.
        let has = slots.read(place, "json_has_field", Type::Bool);
        let read = self.read_fields(held, inner.of, slots)?;
        let optional = Type::Optional(Box::new(read.ty.clone()));
        let some = Typed::new(optional.clone(), Node::Some(Box::new(read)));
        let none = Typed::new(optional, Node::None);
        Ok((Typed::cond(has, some, none), true))
    }
}

/// A field of a decoded document that holds a scalar, in `place`, read from its slot.
fn scalar_field(
    field: &Field,
    nullable: bool,
    place: Place,
    slots: &Slots,
) -> Result<(Typed, bool), Error> {
    scalar_kind(&field.ty)?;
    let of = field.ty.cel();
    let read = |source: Typed, to: Type| {
        let read = Typed::new(to.clone(), Node::FromDyn(Box::new(source), to));
        if field.ty.narrow() {
            super::message::within(read, &field.ty)
        } else {
            Ok(read)
        }
    };
    let get = || slots.read(place, "json_field", Type::Dyn);
    let maybe = Type::Optional(Box::new(Type::Dyn));
    let found = || slots.read(place, "json_optional_field", maybe.clone());
    if nullable && field.ty.narrow() {
        return unsupported("a nullable narrow field in a decode");
    }
    match (nullable, field.required) {
        (false, true) => Ok((read(get(), of)?, false)),
        (true, true) => {
            let some = Typed::new(maybe.clone(), Node::Some(Box::new(get())));
            Ok((read(some, of)?, false))
        }
        (true, false) => Ok((read(found(), of)?, false)),
        (false, false) if field.ty.narrow() => unsupported("an optional narrow field in a decode"),
        (false, false) => Ok((read(found(), Type::Optional(Box::new(of)))?, true)),
    }
}

/// Whether a decode reads a field of type `ty`, nullable or not, from a scalar.
fn scalar_kind(ty: &Type) -> Result<(), Error> {
    let held = match ty {
        Type::Nullable(t) => &**t,
        t => t,
    };
    match held {
        Type::Int
        | Type::Uint
        | Type::Double
        | Type::Bool
        | Type::String
        | Type::Narrow(_)
        | Type::Timestamp
        | Type::Duration
        | Type::Bytes => Ok(()),
        _ => unsupported(format!("a {} field in a decode", held.display())),
    }
}

/// A decode's slots: the document's slot values, and the next slot a field takes.
struct Slots {
    typed: Typed,
    next: usize,
}

/// A field's place in a decode: the slot of the message holding it, and its own.
#[derive(Clone, Copy)]
struct Place {
    of: usize,
    at: usize,
}

impl Slots {
    /// `f` of the field in `place`, typed `ty`.
    fn read(&self, place: Place, f: &str, ty: Type) -> Typed {
        let n = |i: usize| Typed::int(i64::try_from(i).unwrap_or(i64::MAX));
        Typed::call(f, vec![self.typed.clone(), n(place.of), n(place.at)], ty)
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

    /// The rest field a decode node keeps the other keys under.
    fn rest_of(n: &crate::Typed) -> Option<Option<String>> {
        match &n.node {
            crate::Node::Shaped(_, _, rest) => Some(rest.clone()),
            _ => None,
        }
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
            crate::walk::nodes(&t, &mut |n| found = found.take().or(rest_of(n)));
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
