//! Messages: what reading one takes beyond the field itself. cel-emit writes each message type as a
//! `#[repr(C)]` struct, its fields in order, and reads a field directly; presence (`has()`), a
//! field's default, and comparing messages go here.
