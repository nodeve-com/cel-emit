//! A model, as schema-emit lowers it, mapped to the cel-emit record that holds it.
//! Only types map; the checks stay schema-emit's. What cel-emit cannot hold is refused with its
//! reason, never widened.

use cel_emit::{Field, Type, Width};
use schema_emit::ir::{Base, Float, Int, Model, Schema, Ty};
use schema_emit::Error;

#[cfg(test)]
mod tests;

/// Model `root` of `schema` as a record: one field per field, a nested model a nested record, a
/// slot the type filling it. A
/// model wrapping a root that is no object ([`schema_emit::ir::Model::is_value`]) is the type it
/// holds.
pub fn record(schema: &Schema, root: usize) -> Result<Type, Error> {
    let value = schema.models.get(root).filter(|m| m.is_value());
    let mut walk = Walk::new(schema);
    match value.and_then(|m| m.fields.first()) {
        Some(f) => walk.ty(&f.ty),
        None => walk.model(root),
    }
}

/// The type of a value of `t`, as a field of that type holds it, in model `root`: the slots it
/// has, or fills, hold their filling wherever `t` reaches them.
pub fn value(schema: &Schema, root: usize, t: &Ty) -> Result<Type, Error> {
    let mut walk = Walk::new(schema);
    walk.enter(root)?;
    walk.ty(t)
}

/// Model `root` and each model it holds, at any depth, as a record: the type of each, its slots
/// filled as `root` fills them, by the model's index. Nothing where `root` has no record.
pub fn entered(schema: &Schema, root: usize) -> Vec<(usize, Type)> {
    let mut walk = Walk::new(schema);
    walk.entered = Some(Vec::new());
    match walk.model(root) {
        Ok(_) => walk.entered.unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

/// A walk of a schema's types: the models being entered, and what each slot in scope holds.
struct Walk<'a> {
    schema: &'a Schema,
    open: Vec<usize>,
    fills: Vec<(String, Ty)>,
    /// Each model made so far, where the walk keeps them.
    entered: Option<Vec<(usize, Type)>>,
}

impl<'a> Walk<'a> {
    fn new(schema: &'a Schema) -> Self {
        Walk {
            schema,
            open: Vec::new(),
            fills: Vec::new(),
            entered: None,
        }
    }

    /// Enters model `at`, giving the model whose fields it has and the fills in scope before.
    /// A filler holds the fills of the model it fills; a model with slots takes a fill in scope
    /// of the same name, else the slot's own type, so a slot at any depth holds its filling.
    fn enter(&mut self, at: usize) -> Result<(Model, usize), Error> {
        let missing = || Error::invalid(format!("model {at} is not in the schema"));
        let m = self.schema.models.get(at).ok_or_else(missing)?;
        let kept = self.fills.len();
        let (of, fills) = match &m.of {
            Some(f) => {
                let generic = self.schema.models.get(f.model).ok_or_else(missing)?;
                let names = generic.slots.iter().map(|s| s.name.clone());
                (
                    generic,
                    names.zip(f.with.iter().cloned()).collect::<Vec<_>>(),
                )
            }
            None => (
                m,
                m.slots
                    .iter()
                    .map(|s| (s.name.clone(), s.ty.clone()))
                    .collect(),
            ),
        };
        for (name, ty) in fills {
            if m.of.is_some() || !self.fills.iter().any(|(n, _)| *n == name) {
                self.fills.push((name, ty));
            }
        }
        let mut model = of.clone();
        model.name.clone_from(&m.name);
        Ok((model, kept))
    }

    fn model(&mut self, at: usize) -> Result<Type, Error> {
        let (m, kept) = self.enter(at)?;
        let made = self.fields(at, &m);
        self.fills.truncate(kept);
        if let (Ok(made), Some(entered)) = (&made, &mut self.entered) {
            entered.push((at, made.clone()));
        }
        made
    }

    fn fields(&mut self, at: usize, m: &Model) -> Result<Type, Error> {
        if self.open.contains(&at) {
            return Err(Error::unsupported(format!(
                "{}: a model holding itself has no finite record",
                m.name
            )));
        }
        if m.others.is_some() {
            return Err(Error::unsupported(format!(
                "{}: keys no field names have no record",
                m.name
            )));
        }
        self.open.push(at);
        let mut fields = Vec::new();
        for f in &m.fields {
            let ty = self
                .ty(&f.ty)
                .map_err(|e| e.at(&format!("{}.{}", m.name, f.name)))?;
            // A field a record may lack is an option of its type.
            let ty = match f.required {
                true => ty,
                false => Type::Optional(Box::new(ty)),
            };
            fields.push(Field {
                name: f.name.clone(),
                ty,
                required: f.required,
            });
        }
        self.open.pop();
        Ok(Type::message(fields))
    }

    /// The type, a number at its stored width wherever it sits.
    fn ty(&mut self, t: &Ty) -> Result<Type, Error> {
        let base = match &t.base {
            Base::Bool => Type::Bool,
            Base::Text => Type::String,
            Base::Timestamp => Type::Timestamp,
            Base::Duration => Type::Duration,
            Base::Bytes => Type::Bytes,
            Base::Int(i) => int(*i),
            Base::Float(Float::F32) => Type::Narrow(Width::F32),
            Base::Float(Float::F64) => Type::Double,
            Base::List(item) => Type::list(self.ty(item)?),
            Base::Map(item) => Type::map(self.ty(item)?),
            Base::Tuple { prefix, rest } => self.tuple(prefix, rest.as_deref())?,
            Base::Model(m) => self.model(*m)?,
            Base::Null => return Err(Error::unsupported("null alone has no type")),
            Base::Any => return Err(Error::unsupported("any value has no type")),
            Base::Union(_) => return Err(Error::unsupported("unions are not on the edge")),
            Base::Slot(name) => {
                let found = self.fills.iter().rev().find(|(n, _)| n == name);
                let Some((_, fill)) = found.cloned() else {
                    return Err(Error::unsupported("a slot outside any model has no type"));
                };
                // The fill is a type of the schema, outside the slots in scope here.
                let inside = self.fills.split_off(0);
                let made = self.ty(&fill);
                self.fills = inside;
                return made;
            }
        };
        Ok(if t.nullable {
            Type::Nullable(Box::new(base))
        } else {
            base
        })
    }

    /// An array whose positions all share one type is a list; any other mix has no type.
    fn tuple(&mut self, prefix: &[Ty], rest: Option<&Ty>) -> Result<Type, Error> {
        let mut all = prefix.iter().chain(rest);
        let first = all
            .next()
            .ok_or_else(|| Error::unsupported("an empty array has no type"))?;
        if all.any(|t| t != first) {
            return Err(Error::unsupported("a mixed-kind tuple has no type"));
        }
        Ok(Type::list(self.ty(first)?))
    }
}

fn int(i: Int) -> Type {
    match i {
        Int::I8 => Type::Narrow(Width::I8),
        Int::I16 => Type::Narrow(Width::I16),
        Int::I32 => Type::Narrow(Width::I32),
        Int::I64 => Type::Int,
        Int::U8 => Type::Narrow(Width::U8),
        Int::U16 => Type::Narrow(Width::U16),
        Int::U32 => Type::Narrow(Width::U32),
        Int::U64 => Type::Uint,
        Int::I128 => Type::Narrow(Width::I128),
        Int::U128 => Type::Narrow(Width::U128),
    }
}
