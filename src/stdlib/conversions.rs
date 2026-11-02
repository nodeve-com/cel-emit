//! Conversions: cel-emit-runtime's `conversions`.

use crate::functions::Std;

pub(super) const FUNCTIONS: &[Std] = &[
    Std::any("int_to_double", "conversions::int_to_double"),
    Std::any("int_to_uint", "conversions::int_to_uint"),
    Std::any("uint_to_double", "conversions::uint_to_double"),
    Std::any("uint_to_int", "conversions::uint_to_int"),
    Std::any("uint_to_string", "conversions::uint_to_string"),
    Std::any("double_to_uint", "conversions::double_to_uint"),
    Std::reads("string_to_uint", "conversions::string_to_uint"),
    Std::any("uint_to_uint", "conversions::same"),
    Std::any("double_to_int", "conversions::double_to_int"),
    Std::reads("string_to_double", "conversions::string_to_double"),
    Std::reads("string_to_int", "conversions::string_to_int"),
    Std::reads("string_to_bool", "conversions::string_to_bool"),
    Std::any("int_to_string", "conversions::int_to_string"),
    Std::any("double_to_string", "conversions::double_to_string"),
    Std::any("float32_to_string", "conversions::float32_to_string"),
    Std::any("int_to_int", "conversions::same"),
    Std::any("double_to_double", "conversions::same"),
    Std::any("string_to_string", "conversions::same"),
    Std::any("bytes_to_bytes", "conversions::same"),
    Std::reads("string_to_bytes", "conversions::string_to_bytes"),
    Std::reads("bytes_to_string", "conversions::bytes_to_string"),
    Std::any("bool_to_bool", "conversions::same"),
    Std::any("timestamp_to_timestamp", "conversions::same"),
    Std::any("duration_to_duration", "conversions::same"),
    Std::reads("base64_encode_bytes", "conversions::base64_encode"),
    Std::reads("base64_decode_string", "conversions::base64_decode"),
];
