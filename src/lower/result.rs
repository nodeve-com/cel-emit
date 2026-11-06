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
}
