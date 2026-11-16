//! A model, as schema-emit lowers it, mapped to the cel-emit record that holds it.
//! Only types map; the checks stay schema-emit's. What cel-emit cannot hold is refused with its
//! reason, never widened.

use cel_emit::{Field, Type, Width};
use schema_emit::emit::std_form::StdForm;
use schema_emit::ir::{Base, Float, Int, Model, Schema, Ty};
use schema_emit::Error;

/// Model `root` of `schema` as a record: one field per field, a nested model a nested record,
/// each type as schema-emit fills it ([`Schema::filled`]). A model wrapping a root that is no
/// object ([`schema_emit::ir::Model::is_value`]) is the type it holds.
pub fn record(schema: &Schema, root: usize) -> Result<Type, Error> {
    model(&schema.filled(), None, root)
}

/// Model `root` of `std`'s schema as [`record`] gives it, where each field `std` fills later
/// ([`StdForm::with_empty`]) is one that may be empty, as the struct schema-emit writes holds it.
pub fn record_in(std: &StdForm, root: usize) -> Result<Type, Error> {
    model(&std.schema.filled(), Some(std), root)
}

/// Model `root` of `filled`, a filled schema, as [`record_in`] gives it, or [`record`] with no
/// `std`.
pub(crate) fn model(filled: &Schema, std: Option<&StdForm>, root: usize) -> Result<Type, Error> {
    let value = filled.models.get(root).filter(|m| m.is_value());
    let mut walk = Walk::new(filled, std);
    match value.and_then(|m| m.fields.first()) {
        Some(f) => walk.ty(&f.ty),
        None => walk.model(root),
    }
}

/// The type of a value of `t`, a type of `filled`, a filled schema, as [`model`] gives it.
pub(crate) fn value(filled: &Schema, std: Option<&StdForm>, t: &Ty) -> Result<Type, Error> {
    Walk::new(filled, std).ty(t)
}

/// A walk of a filled schema's types: the form that fills fields later, and the models being
/// entered.
struct Walk<'a> {
    schema: &'a Schema,
    std: Option<&'a StdForm<'a>>,
    open: Vec<usize>,
}

impl<'a> Walk<'a> {
    fn new(schema: &'a Schema, std: Option<&'a StdForm<'a>>) -> Self {
        Walk {
            schema,
            std,
            open: Vec::new(),
        }
    }

    fn model(&mut self, at: usize) -> Result<Type, Error> {
        let missing = || Error::invalid(format!("model {at} is not in the schema"));
        let schema = self.schema;
        let m = schema.models.get(at).ok_or_else(missing)?;
        self.fields(at, m)
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
        for (i, f) in m.fields.iter().enumerate() {
            let ty = self
                .ty(&f.ty)
                .map_err(|e| e.at(&format!("{}.{}", m.name, f.name)))?;
            // A field a record may lack, or the form fills later, is an option of its type.
            let held = f.required && !self.later(at, i);
            let ty = match held {
                true => ty,
                false => Type::Optional(Box::new(ty)),
            };
            fields.push(Field {
                name: f.name.clone(),
                ty,
                required: held,
            });
        }
        self.open.pop();
        Ok(Type::message(fields))
    }

    /// Whether the form fills field `field` of model `at` later, and no default holds it.
    fn later(&self, at: usize, field: usize) -> bool {
        let std = self.std;
        std.is_some_and(|s| s.can_be_empty(self.schema.filling(at), field))
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
            _ => return Err(Error::unsupported("a type left open has no type")),
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
