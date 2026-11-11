//! How an expression reads each variable: a property of the record, a property of the params
//! where they are a record, or one of the caller's params.

use super::{Computed, Property, Record};
use crate::decls::{Decls, Name};
use crate::emit::Vars;
use crate::functions::STD;
use crate::types::Type;
use crate::Error;

/// The params each entry's function takes beside the record, as `p`.
#[derive(Clone, Copy, Debug)]
pub enum Params<'a> {
    /// A Rust type the caller names, each of its params read as [`Computed::vars`] says.
    Rust(&'a str),
    /// A record, its Rust type the params': an expression reads each of its properties as
    /// `name.key`, as the record's own are read.
    Record { name: &'a str, record: &'a Record },
}

impl<'a> Params<'a> {
    /// The params' Rust type.
    pub(super) fn rust(self) -> &'a str {
        match self {
            Params::Rust(rust) => rust,
            Params::Record { record, .. } => &record.rust,
        }
    }

    /// The property variable `var` names, the params' properties numbered from `first`.
    fn property(self, first: usize, var: usize) -> Option<&'a Property> {
        let Params::Record { record, .. } = self else {
            return None;
        };
        record.properties.get(var.checked_sub(first)?)
    }

    /// Each of the params' properties into `decls` as `name.key`, numbered from `first`. A name
    /// already there that is `name`, or starts with it and a dot, is an error.
    pub(super) fn declare(self, decls: &mut Decls, first: usize) -> Result<(), Error> {
        let Params::Record { name, record } = self else {
            return Ok(());
        };
        let inside = format!("{name}.");
        let mut names = decls.names.keys();
        if let Some(taken) = names.find(|k| *k == name || k.starts_with(&inside)) {
            return Err(Error::Invalid("a name two things share".into()).at(taken));
        }
        for (i, property) in record.properties.iter().enumerate() {
            let var = Name::var(property.ty.clone(), first.saturating_add(i));
            decls.names.insert(format!("{inside}{}", property.key), var);
        }
        Ok(())
    }
}

/// How an expression reads each variable: a property from the record `r`, one of the params'
/// from `p`, any other as the caller says, over `r` and `p`.
pub(super) struct Reads<'a> {
    pub(super) computed: &'a Computed<'a>,
    pub(super) first: usize,
}

impl Vars for Reads<'_> {
    fn read(&self, var: usize) -> Result<String, Error> {
        let computed = self.computed;
        if let Some(property) = computed.property(self.first, var) {
            return Ok(property.read("r"));
        }
        let past = self.first.saturating_add(computed.record.properties.len());
        match computed.params.property(past, var) {
            Some(property) => Ok(property.read("p")),
            None => computed.vars.read(var),
        }
    }
}

impl Property {
    /// Its read from `base`, the record `r` or the params `p`: an optional as one, a message's by
    /// reference; any other its value, raising the absence that skips the expression where it is
    /// empty.
    fn read(&self, base: &str) -> String {
        let place = &self.place;
        match (&self.ty, self.empty) {
            (Type::Optional(held), _) if matches!(**held, Type::Message(_)) => {
                format!("{base}.{place}.as_ref()")
            }
            (Type::Optional(_), _) => format!("{base}.{place}.clone()"),
            (_, true) => format!("(*{base}.{place}.as_ref().ok_or({STD}::Error::Absent)?)"),
            (_, false) => format!("{base}.{place}"),
        }
    }
}
