//! A stored number's width.

use super::Type;

/// A number's width where it is not CEL's own (`int64`, `uint64`, `float64`): it is stored as
/// that Rust type, a write outside it is an error, and a read widens to [`Width::cel`] where what
/// takes it computes in CEL's width.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Width {
    I8,
    I16,
    I32,
    I128,
    U8,
    U16,
    U32,
    U128,
    F32,
}

impl Width {
    /// The Rust type a message stores.
    pub fn rust(self) -> &'static str {
        match self {
            Width::I8 => "i8",
            Width::I16 => "i16",
            Width::I32 => "i32",
            Width::I128 => "i128",
            Width::U8 => "u8",
            Width::U16 => "u16",
            Width::U32 => "u32",
            Width::U128 => "u128",
            Width::F32 => "f32",
        }
    }

    /// The CEL type a field of this width computes in.
    pub fn cel(self) -> Type {
        match self {
            Width::I8 | Width::I16 | Width::I32 | Width::I128 => Type::Int,
            Width::U8 | Width::U16 | Width::U32 | Width::U128 => Type::Uint,
            Width::F32 => Type::Double,
        }
    }
}
