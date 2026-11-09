//! An expression given the type it returns.

use super::*;

/// `source`, checked against `decls` and lowered as a value of `ty`, the way a message's field of
/// `ty` takes a value: a stored width holds, so a number outside it is a range error when the
/// tree runs, element by element in a list or map. A dyn fills it as it fills a field. The tree's
/// type is `ty`, and [`crate::emit::Emitter::rust`] gives its Rust.
pub fn lower_as(source: &str, decls: &Decls, ty: &Type) -> Result<Typed, Error> {
    let at = match ty {
        Type::Map(..) => Root::Map,
        _ => Root::Value,
    };
    let value = lowered_at(source, decls, at)?.tree;
    let value = super::nullable::fit(value, &ty.cel())?;
    let value = super::message::held(value, ty);
    if !ty.narrow() || value.node == Node::None {
        return Ok(Typed {
            ty: ty.clone(),
            ..value
        });
    }
    crate::walk::cast(value, ty)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Width;

    fn lowered_as(source: &str, ty: &Type) -> Typed {
        lower_as(source, &Decls::default(), ty).unwrap()
    }

    #[test]
    fn an_optional_or_nullable_result_holds_its_inner_width() {
        let int8 = Box::new(Type::Narrow(Width::I8));
        for ty in [Type::Optional(int8.clone()), Type::Nullable(int8)] {
            assert_eq!(lowered_as("1", &ty).ty, ty);
        }
        let nullable = Type::Nullable(Box::new(Type::Narrow(Width::I8)));
        assert_eq!(lowered_as("null", &nullable).node, Node::None);
        let optional = Type::Optional(Box::new(Type::Narrow(Width::I8)));
        assert_eq!(lowered_as("optional.of(1)", &optional).ty, optional);
    }

    #[test]
    fn a_result_of_another_type_is_refused() {
        let int8 = Type::Narrow(Width::I8);
        assert!(lower_as("'a'", &Decls::default(), &int8).is_err());
    }

    /// `c`, a bool, and `x`, `y`, `f`, `g`, `o` stored as int32, int32, float32, float32, and an
    /// optional float32; `half` takes a float32.
    fn stored() -> Decls {
        let f32 = Type::Narrow(Width::F32);
        let mut d = Decls::default();
        let names = [
            ("c", Type::Bool),
            ("x", Type::Narrow(Width::I32)),
            ("y", Type::Narrow(Width::I32)),
            ("f", f32.clone()),
            ("g", f32.clone()),
            ("o", Type::Optional(Box::new(f32.clone()))),
        ];
        for (i, (name, ty)) in names.into_iter().enumerate() {
            d.names.insert(name.into(), crate::Name::var(ty, i));
        }
        d.functions.push(crate::functions::Function {
            decl: crate::Decl::new("half").with_overload("half_f32", vec![f32], Type::Double),
            paths: vec![("half_f32".into(), "half".into())],
        });
        d
    }

    /// Whether `t` widens, narrows, or holds a value to a width anywhere.
    fn converts(t: &Typed) -> bool {
        crate::walk::any(t, |n| {
            matches!(n, Node::Widen(_) | Node::Store(_) | Node::Within(..))
        })
    }

    #[test]
    fn a_stored_value_passes_through_at_its_width() {
        let d = stored();
        let f32 = Type::Narrow(Width::F32);
        let i32 = Type::Narrow(Width::I32);
        for (source, ty) in [
            ("f", f32.clone()),
            ("c ? f : g", f32.clone()),
            ("c ? x : y", i32.clone()),
            ("o.orValue(f)", f32.clone()),
            ("optional.of(f)", Type::Optional(Box::new(f32.clone()))),
            ("[x, y]", Type::list(i32.clone())),
            ("x == y", Type::Bool),
            ("f < g", Type::Bool),
            ("x >= y", Type::Bool),
            ("half(c ? f : g)", Type::Double),
        ] {
            let t = lower_as(source, &d, &ty).unwrap();
            assert!(!converts(&t), "{source}: {t:?}");
        }
    }

    #[test]
    fn arithmetic_and_mixed_widths_widen() {
        let d = stored();
        for source in ["x + y", "x == 1", "f < 0.5", "c ? f : 0.5", "[x, 1]"] {
            assert!(converts(&crate::lower(source, &d).unwrap()), "{source}");
        }
    }
}
