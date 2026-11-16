//! A tuple, an array whose `prefixItems` type its positions, as cel-emit's opaque type: the Rust
//! tuple schema-emit writes for it, and the two functions reading and writing one as a dynamic
//! value, made of schema-emit's own reader and writer of that type.

use std::collections::BTreeMap;

use schema_emit::emit::std_form::StdForm;
use schema_emit::ir::{Base, Schema, Ty};
use schema_emit::Error;

/// A tuple type a schema holds, and the functions [`crate::Bridged::items`] holds for it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tuple {
    /// The tuple's Rust type, the text of its [`cel_emit::Type::Opaque`].
    pub rust: String,
    /// The Rust path of the function writing one as a dynamic value, as
    /// [`cel_emit::emit::Messages::writes`] names it.
    pub writes: String,
    /// The Rust path of the function reading one from a dynamic value, as
    /// [`cel_emit::emit::Messages::reads`] names it.
    pub reads: String,
}

/// Refuses a tuple type whose Rust text no code outside the module holding its items can name,
/// or that is left open.
fn apart(ty: &Ty) -> Result<(), Error> {
    match &ty.base {
        Base::List(item) | Base::Map(item) => apart(item),
        Base::Tuple { prefix, rest } => prefix.iter().chain(rest.as_deref()).try_for_each(apart),
        Base::Model(_) | Base::Union(_) => Err(Error::unsupported(
            "a tuple holding a model or a union has no type",
        )),
        Base::DynamicRef(_) => Err(Error::unsupported("a type left open has no type")),
        _ => Ok(()),
    }
}

/// The Rust type of `ty`, a tuple type of `schema`, null apart: `std`'s text for it.
pub(crate) fn rust(schema: &Schema, std: Option<&StdForm>, ty: &Ty) -> Result<String, Error> {
    apart(ty)?;
    let whole = Ty::of(ty.base.clone());
    Ok(match std {
        Some(std) => std.rust(&whole),
        // The text of a tuple holding no model takes no model's name.
        None => StdForm::new(schema, &|_| String::new()).rust(&whole),
    })
}

/// A name for the Rust type `rust`, the same wherever that type is met: FNV-1a over its text.
fn id(rust: &str) -> String {
    let hash = rust.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    });
    format!("{hash:016x}")
}

/// Each tuple type `ty` holds outside any other tuple, by its Rust text.
fn held(schema: &Schema, std: &StdForm, ty: &Ty, out: &mut BTreeMap<String, Ty>) {
    match &ty.base {
        Base::List(item) | Base::Map(item) => held(schema, std, item, out),
        Base::Tuple { .. } => {
            if let Ok(text) = rust(schema, Some(std), ty) {
                out.insert(text, Ty::of(ty.base.clone()));
            }
        }
        _ => {}
    }
}

/// Each distinct tuple type `filled`, `std`'s schema filled, holds in a field or names, in the
/// order of its Rust text.
fn met(filled: &Schema, std: &StdForm) -> BTreeMap<String, Ty> {
    let mut out = BTreeMap::new();
    let fields = filled.models.iter().flat_map(|m| m.fields.iter());
    for ty in fields
        .map(|f| &f.ty)
        .chain(filled.named.iter().map(|(_, ty)| ty))
    {
        held(filled, std, ty, &mut out);
    }
    out
}

/// Each tuple type of `filled`, its functions in `module`.
pub(crate) fn tuples(filled: &Schema, std: &StdForm, module: &str) -> Vec<Tuple> {
    let each = met(filled, std).into_keys().map(|rust| {
        let id = id(&rust);
        Tuple {
            writes: format!("{module}::write_tuple_{id}"),
            reads: format!("{module}::read_tuple_{id}"),
            rust,
        }
    });
    each.collect()
}

/// The functions [`tuples`] names: each holds schema-emit's writer or reader of its type, and
/// gives or takes the dynamic value through JSON text.
pub(crate) fn items(filled: &Schema, std: &StdForm) -> Result<String, Error> {
    let mut out = String::new();
    for (rust, ty) in met(filled, std) {
        let id = id(&rust);
        let (write, read) = (
            std.encode_value(&ty, "write")?,
            std.decode_value(&ty, "tuple", "read")?,
        );
        out.push_str(&format!(
            "
pub fn write_tuple_{id}(v: &{rust}) -> ::cel_emit_runtime::Result<::cel_emit_runtime::Value> {{
    {write}
    struct Held<'a>(&'a {rust});
    impl __s::decode::Serialize for Held<'_> {{
        fn serialize<S: __s::decode::Serializer>(&self, s: S)
            -> ::core::result::Result<S::Ok, S::Error> {{ write(self.0, s) }}
    }}
    ::cel_emit_runtime::json::serialized(&Held(v))
}}
pub fn read_tuple_{id}(x: &::cel_emit_runtime::Value) -> ::cel_emit_runtime::Result<{rust}> {{
    {read}
    let text = ::cel_emit_runtime::json::encode(x)?;
    __s::decode::json(&text, |d| read(d))
        .map(|(v, _)| v)
        .map_err(|_| ::cel_emit_runtime::Error::Cast)
}}
"
        ));
    }
    Ok(out)
}
