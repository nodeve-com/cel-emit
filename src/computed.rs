//! # In Rust
//!
//! A [`Record`](crate::computed::Record) says what the caller gives;
//! [`Computed::index`](crate::computed::Computed::index) writes the index, a `static` of
//! cel-emit-runtime's `computed::Entry`, the functions inside it. The caller writes the crate
//! around it, as for any expression ([`crate::emit`]).
//! [`Computed::decls`](crate::computed::Computed::decls) is what an expression checks against, for
//! a caller lowering its own tree.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::decls::{At, Decls, Name};
use crate::emit::{Emitter, Vars};
use crate::functions::STD;
use crate::tree::Typed;
use crate::types::Type;
use crate::{walk, Error};

mod order;
mod reads;

pub use reads::Params;
use reads::Reads;

/// One property of the record: what an expression reads by `key`.
#[derive(Clone, Debug)]
pub struct Property {
    pub key: String,
    /// Its type. An optional reads as one, empty or not; any other reads as its value, and an
    /// expression reading it empty doesn't run.
    pub ty: Type,
    /// Where it sits: the Rust path of its field from the record, as `total` or `data.total`.
    pub place: String,
    /// Whether its place may be empty: an `Option` of its type's Rust. An optional's always may.
    pub empty: bool,
    /// Its check: the Rust path of a function taking the value and returning it, or
    /// cel-emit-runtime's `computed::Failure`.
    pub check: Option<String>,
}

/// The record computed properties fill.
#[derive(Clone, Debug)]
pub struct Record {
    /// Its Rust type.
    pub rust: String,
    pub properties: Vec<Property>,
}

/// What fills a computed property.
#[derive(Clone, Debug)]
pub enum Expression {
    /// CEL text.
    Cel(String),
    /// A tree of the property's type, lowered against [`Computed::decls`].
    Tree(Typed),
}

/// One computed property: the property `key` names, and the expression filling it.
#[derive(Clone, Debug)]
pub struct Compute {
    pub key: String,
    pub expression: Expression,
    /// Properties it fills after, beside the ones its expression reads.
    pub after: Vec<String>,
    /// Whether the record needs it, which its entry repeats for the walk to judge by.
    pub required: bool,
}

/// A record, and what its expressions read and call beside it: the caller's declarations, its
/// params, and how each of the caller's variables reads.
pub struct Computed<'a> {
    pub record: &'a Record,
    pub decls: &'a Decls,
    /// The params: what each entry's function takes beside the record.
    pub params: Params<'a>,
    /// How each of the caller's variables reads: Rust over the params `p` and the record `r`.
    /// One using a property an expression fills needs it in [`Compute::after`].
    pub vars: &'a dyn Vars,
}

/// A computed property ready to emit.
struct Fill<'a> {
    property: &'a Property,
    compute: &'a Compute,
    tree: Typed,
    /// The properties its tree reads, in key order.
    reads: Vec<&'a str>,
}

impl Property {
    /// The type of the value an expression gives it.
    fn value(&self) -> &Type {
        match &self.ty {
            Type::Optional(held) => held,
            ty => ty,
        }
    }

    /// Whether its place is an `Option` of its value.
    fn optional(&self) -> bool {
        self.empty || matches!(self.ty, Type::Optional(_))
    }
}

impl<'a> Computed<'a> {
    /// The first variable naming a property: past each of the caller's.
    fn first(&self) -> usize {
        let own = self.decls.names.values().filter_map(|name| match name.at {
            At::Var(id) => Some(id.saturating_add(1)),
            At::Constant(_) => None,
        });
        own.max().unwrap_or(0)
    }

    /// The property variable `var` names, the properties numbered from `first`.
    fn property(&self, first: usize, var: usize) -> Option<&'a Property> {
        let at = var.checked_sub(first)?;
        self.record.properties.get(at)
    }

    /// What an expression checks against: the caller's declarations, each property by its key,
    /// and each of a params record's by its name and key. A key naming two things is an error.
    pub fn decls(&self) -> Result<Decls, Error> {
        let mut decls = self.decls.clone();
        let first = self.first();
        for (i, property) in self.record.properties.iter().enumerate() {
            let name = Name::var(property.ty.clone(), first.saturating_add(i));
            if decls.names.insert(property.key.clone(), name).is_some() {
                return Err(Error::Invalid("a name two things share".into()).at(&property.key));
            }
        }
        let past = first.saturating_add(self.record.properties.len());
        self.params.declare(&mut decls, past)?;
        Ok(decls)
    }

    /// `compute` lowered as its property's value.
    fn fill(&self, compute: &'a Compute, decls: &Decls) -> Result<Fill<'a>, Error> {
        let found = self.record.properties.iter().find(|p| p.key == compute.key);
        let property = found.ok_or_else(|| Error::Invalid("no such property".into()))?;
        let tree = match &compute.expression {
            Expression::Cel(source) => crate::lower_as(source, decls, property.value())?,
            Expression::Tree(tree) if tree.ty == *property.value() => tree.clone(),
            Expression::Tree(_) => return Err(Error::Invalid("a tree of another type".into())),
        };
        let first = self.first();
        let read = walk::vars(&tree).into_iter();
        let mut reads: Vec<&str> = read
            .filter_map(|var| self.property(first, var))
            .map(|p| p.key.as_str())
            .collect();
        reads.sort_unstable();
        reads.dedup();
        Ok(Fill {
            property,
            compute,
            tree,
            reads,
        })
    }

    /// Each of `computes` by its key, lowered. Two filling one property is an error, as is an
    /// `after` naming none.
    fn fills(&self, computes: &'a [Compute]) -> Result<BTreeMap<&'a str, Fill<'a>>, Error> {
        let decls = self.decls()?;
        let mut fills = BTreeMap::new();
        for compute in computes {
            let key = compute.key.as_str();
            let fill = self.fill(compute, &decls).map_err(|e| e.at(key))?;
            let known = |k: &String| self.record.properties.iter().any(|p| p.key == *k);
            if let Some(unknown) = compute.after.iter().find(|k| !known(k)) {
                return Err(Error::Invalid(format!("after {unknown}, no property")).at(key));
            }
            if fills.insert(key, fill).is_some() {
                return Err(Error::Invalid("computed twice".into()).at(key));
            }
        }
        Ok(fills)
    }

    /// The index of `computes` as the `static` `name`: an entry per computed property in the
    /// order they fill, and inside it each entry's function, emitted by `emitter`. Declare
    /// `name` where the record's type and `use ::cel_emit_runtime as s;` are in scope.
    pub fn index(
        &self,
        name: &str,
        computes: &'a [Compute],
        emitter: &mut Emitter,
    ) -> Result<String, Error> {
        let fills = self.fills(computes)?;
        let waits = fills.iter().map(|(key, fill)| (*key, fill.waits(&fills)));
        let order = order::fill_order(&waits.collect())?;
        let reads = Reads {
            computed: self,
            first: self.first(),
        };
        let (mut functions, mut entries) = (String::new(), String::new());
        for (n, fill) in order.iter().filter_map(|key| fills.get(key)).enumerate() {
            let (rust, params) = (&self.record.rust, self.params.rust());
            let body = fill
                .body(emitter, &reads)
                .map_err(|e| e.at(&fill.compute.key))?;
            let _ = writeln!(
                functions,
                "#[allow(unused_variables)]
                fn f{n}(record: &mut {rust}, p: &{params}) -> {STD}::computed::Outcome {{ {body} }}"
            );
            let _ = writeln!(entries, "{},", fill.entry(n));
        }
        let (rust, params, count) = (&self.record.rust, self.params.rust(), order.len());
        Ok(format!(
            "pub static {name}: [{STD}::computed::Entry<{rust}, {params}>; {count}] = {{\n{functions}[{entries}] }};\n"
        ))
    }
}

impl<'a> Fill<'a> {
    /// The computed properties it fills after: those it reads, and its `after`.
    fn waits(&self, fills: &BTreeMap<&'a str, Fill<'a>>) -> Vec<&'a str> {
        let after = self.compute.after.iter().map(String::as_str);
        let all = self.reads.iter().copied().chain(after);
        all.filter_map(|key| fills.get_key_value(key).map(|(key, _)| *key))
            .collect()
    }

    /// Its function's body: compute over the record and the params, check, store. `at` holds the place a failed
    /// expression names: the key, or the field of a struct literal it was computing.
    fn body(&self, emitter: &mut Emitter, reads: &Reads) -> Result<String, Error> {
        let Property { key, place, .. } = self.property;
        let ty = emitter.rust(self.property.value())?;
        emitter.at = Some(key.clone());
        let value = emitter.expr(&self.tree, reads);
        emitter.at = None;
        let value = value?;
        let check = self
            .property
            .check
            .as_ref()
            .map_or_else(String::new, |check| {
                format!(
                    "let value = match {check}(value) {{ Ok(value) => value, \
                 Err(failure) => return {STD}::computed::Outcome::Failed(failure) }};"
                )
            });
        let stored = match self.property.optional() {
            true => "Some(value)",
            false => "value",
        };
        Ok(format!(
            "let at = &::core::cell::Cell::new({key:?});
            let value = {{ let r = &*record; (|| -> {STD}::Result<{ty}> {{ Ok({value}) }})() }};
            let value = match {STD}::computed::computed(at.get(), value) {{
                Ok(value) => value,
                Err(outcome) => return outcome,
            }};
            {check}
            record.{place} = {stored};
            {STD}::computed::Outcome::Filled"
        ))
    }

    /// Its entry, its function the `n`th.
    fn entry(&self, n: usize) -> String {
        let (key, reads, required) = (&self.compute.key, &self.reads, self.compute.required);
        format!(
            "{STD}::computed::Entry {{ key: {key:?}, reads: &{reads:?}, required: {required}, fill: f{n} }}"
        )
    }
}
