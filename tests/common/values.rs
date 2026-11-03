//! A value as the Rust that supplies it, and as a native program writes its own results, so a
//! CEL value and a run's answer compare as text.

use cel_core::{MapKey, Value, ValueMap};
use cel_emit::emit::Emitter;
use cel_emit::{Field, Type, Width};

/// Whether a binding or a result of `ty` can be supplied or shown: scalars, bytes, times, and
/// lists and string-keyed maps of them. A stored width is shown as CEL reads it.
pub fn held(ty: &Type) -> bool {
    match ty {
        Type::List(t) => held(t),
        Type::Map(k, v) => **k == Type::String && held(v),
        Type::Narrow(w) => !matches!(w, cel_emit::Width::I128 | cel_emit::Width::U128),
        Type::Int | Type::Uint | Type::Double => true,
        Type::Bool | Type::String | Type::Bytes => true,
        Type::Timestamp | Type::Duration => true,
        _ => false,
    }
}

/// Whether a result of `ty` can be shown: what [`held`] takes, and messages in lists, which show
/// only that they are messages.
pub fn shown(ty: &Type) -> bool {
    match ty {
        Type::List(t) => matches!(**t, Type::Message(_)) || shown(t),
        Type::Map(k, v) => **k == Type::String && shown(v),
        other => held(other),
    }
}

const NANOS: i128 = 1_000_000_000;

/// An undeclared binding's type, from its value: a list's or map's from its first entry, or
/// `empty` where it has none.
pub fn infer(v: &Value, empty: Option<&Type>) -> Option<Type> {
    let of = |first: Option<&Value>| match first {
        Some(first) => infer(first, empty),
        None => empty.cloned(),
    };
    Some(match v {
        Value::Int(_) => Type::Int,
        Value::UInt(_) => Type::Uint,
        Value::Double(_) => Type::Double,
        Value::Bool(_) => Type::Bool,
        Value::String(_) => Type::String,
        Value::Bytes(_) => Type::Bytes,
        Value::Timestamp(_) => Type::Timestamp,
        Value::Duration(_) => Type::Duration,
        Value::List(items) => Type::list(of(items.first())?),
        Value::Map(m) => Type::map(of(m.values().next())?),
        _ => return None,
    })
}

/// A timestamp's seconds and nanoseconds, if CEL holds it.
fn instant(v: &Value) -> Option<(i64, u32)> {
    let Value::Timestamp(t) = v else { return None };
    let nanos = t.to_nanos()?;
    let at = (
        i64::try_from(nanos.div_euclid(NANOS)).ok()?,
        u32::try_from(nanos.rem_euclid(NANOS)).ok()?,
    );
    cel_emit_runtime::time::timestamp_at(at.0, at.1).ok()?;
    Some(at)
}

/// A duration's seconds and nanoseconds, if CEL holds it.
fn span(v: &Value) -> Option<(i64, i32)> {
    let Value::Duration(d) = v else { return None };
    let nanos = i128::from(d.to_nanos());
    let at = (
        i64::try_from(nanos / NANOS).ok()?,
        i32::try_from(nanos % NANOS).ok()?,
    );
    cel_emit_runtime::time::duration_at(at.0, at.1).ok()?;
    Some(at)
}

/// Whether `v` is a value of `ty`.
pub fn fits(v: &Value, ty: &Type) -> bool {
    match (ty, v) {
        (Type::Int, Value::Int(_))
        | (Type::Uint, Value::UInt(_))
        | (Type::Double, Value::Double(_))
        | (Type::Bool, Value::Bool(_))
        | (Type::String, Value::String(_))
        | (Type::Bytes, Value::Bytes(_)) => true,
        (Type::Timestamp, v) => instant(v).is_some(),
        (Type::Duration, v) => span(v).is_some(),
        (Type::List(t), Value::List(items)) => items.iter().all(|i| fits(i, t)),
        (Type::Narrow(w), v) => fits(v, &w.cel()),
        (Type::Map(k, t), Value::Map(m)) => m
            .iter()
            .all(|(key, v)| fits(&key_value(key), k) && fits(v, t)),
        (Type::Message(fields), Value::Map(m)) => m.iter().all(|(key, v)| {
            fields.iter().any(|f| {
                matches!(key, MapKey::String(k) if **k == *f.name) && fits(v, inner(&f.ty))
            })
        }),
        _ => false,
    }
}

/// A field's type with the optional or nullable wrapper off.
fn inner(ty: &Type) -> &Type {
    match ty {
        Type::Optional(t) | Type::Nullable(t) => t,
        t => t,
    }
}

/// A map key as the value it is.
fn key_value(key: &MapKey) -> Value {
    match key {
        MapKey::Bool(b) => Value::Bool(*b),
        MapKey::Int(i) => Value::Int(*i),
        MapKey::UInt(u) => Value::UInt(*u),
        MapKey::String(s) => Value::String(s.clone()),
    }
}

/// `v`, a value of `ty` that [`fits`], as a Rust expression of the type cel-emit writes for it.
pub fn rust(v: &Value, ty: &Type, em: &mut Emitter) -> String {
    match (ty, v) {
        (Type::List(t), Value::List(items)) => list(items, t, em),
        (Type::Map(..), Value::Map(m)) => map(m, ty, em),
        (Type::Message(fields), Value::Map(m)) => message(m, fields, ty, em),
        _ => scalar(v, ty).unwrap_or_else(|| unreachable!("{v:?} as {ty:?}")),
    }
}

/// `v`, a scalar of `ty`, as Rust; `None` for any other pair.
fn scalar(v: &Value, ty: &Type) -> Option<String> {
    Some(match (ty, v) {
        (Type::Int, Value::Int(i)) => format!("({i}i64)"),
        (Type::Uint, Value::UInt(u)) => format!("({u}u64)"),
        (Type::Double, Value::Double(d)) => format!("f64::from_bits({:#x})", d.to_bits()),
        (Type::Bool, Value::Bool(b)) => b.to_string(),
        (Type::Narrow(Width::F32), Value::Double(d)) => {
            format!("(f64::from_bits({:#x}) as f32)", d.to_bits())
        }
        (Type::Narrow(w), Value::Int(i)) => format!("({i}{})", w.rust()),
        (Type::Narrow(w), Value::UInt(u)) => format!("({u}{})", w.rust()),
        (Type::String, Value::String(s)) => format!("String::from({:?})", &**s),
        (Type::Bytes, Value::Bytes(b)) => format!("Vec::<u8>::from(&{:?}[..])", b.to_vec()),
        (Type::Timestamp, v) => {
            let (s, n) = instant(v).unwrap();
            format!("s::time::timestamp_at({s}i64, {n}u32)?")
        }
        (Type::Duration, v) => {
            let (s, n) = span(v).unwrap();
            format!("s::time::duration_at({s}i64, {n}i32)?")
        }
        _ => return None,
    })
}

/// The Rust type of `t`.
fn of(t: &Type, em: &mut Emitter) -> String {
    em.rust(t).unwrap()
}

/// A list `items` of `t`, as Rust.
fn list(items: &[Value], t: &Type, em: &mut Emitter) -> String {
    if items.is_empty() {
        return format!("Vec::<{}>::new()", of(t, em));
    }
    let items: Vec<String> = items.iter().map(|i| rust(i, t, em)).collect();
    format!("Vec::from([{}])", items.join(", "))
}

/// A map `m` of the map type `ty`, as Rust.
fn map(m: &ValueMap, ty: &Type, em: &mut Emitter) -> String {
    let Type::Map(k, t) = ty else {
        unreachable!("{ty:?} is no map")
    };
    if m.is_empty() {
        return format!("Vec::<({}, {})>::new()", of(k, em), of(t, em));
    }
    let entries: Vec<String> = m
        .iter()
        .map(|(key, v)| {
            let (key, value) = (rust(&key_value(key), k, em), rust(v, t, em));
            format!("({key}, {value})")
        })
        .collect();
    format!("Vec::from([{}])", entries.join(", "))
}

/// A message `ty` of `fields`, set from the map `m`, as Rust.
fn message(m: &ValueMap, fields: &[Field], ty: &Type, em: &mut Emitter) -> String {
    let mut body = format!("let mut rec = {}::default(); ", of(ty, em));
    for (n, f) in fields.iter().enumerate() {
        let set = m
            .iter()
            .find(|(k, _)| matches!(k, MapKey::String(k) if **k == *f.name));
        if let Some((_, v)) = set {
            let value = rust(v, inner(&f.ty), em);
            let value = match f.ty {
                Type::Optional(_) | Type::Nullable(_) => format!("Some({value})"),
                _ => value,
            };
            body.push_str(&format!("rec.f{n} = {value}; "));
        }
    }
    format!("{{ {body}rec }}")
}

/// How a double is written: `0.0` for either zero.
pub fn double(d: f64) -> String {
    if d == 0.0 {
        "0.0".into()
    } else {
        format!("{d:?}")
    }
}

/// `v` as a native program writes a result of its type; what it can't hold is `<other>`.
pub fn show(v: &Value) -> String {
    show_with(v, double)
}

/// [`show`], writing each double as `double` does.
pub fn show_with(v: &Value, double: fn(f64) -> String) -> String {
    let show = |v: &Value| show_with(v, double);
    match v {
        Value::Int(v) => format!("{v}"),
        Value::UInt(v) => format!("{v}u"),
        Value::Double(v) => double(*v),
        Value::Bool(v) => format!("{v}"),
        Value::String(v) => format!("{:?}", &**v),
        Value::Bytes(v) => format!("b{:?}", v.to_vec()),
        Value::Timestamp(_) => instant(v).map_or("<other>".into(), |(s, n)| format!("t{s}.{n}")),
        Value::Duration(_) => span(v).map_or("<other>".into(), |(s, n)| format!("d{s}.{n}")),
        Value::List(xs) => {
            let xs: Vec<String> = xs.iter().map(show).collect();
            format!("[{}]", xs.join(","))
        }
        Value::Map(m) => {
            let mut entries: Vec<String> = m
                .iter()
                .map(|(k, v)| match k {
                    MapKey::String(k) => format!("{:?}:{}", &**k, show(v)),
                    other => format!("{other:?}:{}", show(v)),
                })
                .collect();
            entries.sort();
            format!("{{{}}}", entries.join(","))
        }
        _ => "<other>".into(),
    }
}
