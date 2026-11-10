//! The order computed properties fill in: each after the ones it waits on.

use std::collections::BTreeMap;

use crate::Error;

/// The keys of `waits`, each after the keys it waits on, the smallest ready key first; a cycle is
/// an error naming it.
pub(super) fn fill_order<'a>(
    waits: &BTreeMap<&'a str, Vec<&'a str>>,
) -> Result<Vec<&'a str>, Error> {
    let mut done: Vec<&str> = Vec::with_capacity(waits.len());
    while done.len() < waits.len() {
        let ready = waits
            .iter()
            .find(|(key, on)| !done.contains(key) && on.iter().all(|k| done.contains(k)));
        match ready {
            Some((key, _)) => done.push(key),
            None => return Err(cycle(waits, &done)),
        }
    }
    Ok(done)
}

/// A cycle among the keys not `done`, each of which waits on another of them.
fn cycle(waits: &BTreeMap<&str, Vec<&str>>, done: &[&str]) -> Error {
    let left = |key: &&str| !done.contains(key);
    let mut path: Vec<&str> = waits.keys().copied().filter(left).take(1).collect();
    loop {
        let next = path
            .last()
            .and_then(|key| waits.get(key))
            .and_then(|on| on.iter().copied().find(left));
        let Some(next) = next else { break };
        let seen = path.iter().position(|key| *key == next);
        path.push(next);
        if let Some(at) = seen {
            path.drain(..at);
            break;
        }
    }
    Error::Invalid(format!(
        "computed properties depend on each other: {}",
        path.join(" -> ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn order(waits: &[(&'static str, &[&'static str])]) -> Result<Vec<&'static str>, Error> {
        let waits = waits.iter().map(|(k, on)| (*k, on.to_vec())).collect();
        fill_order(&waits)
    }

    #[test]
    fn a_property_fills_after_what_it_waits_on() {
        let got = order(&[
            ("label", &["order", "total"]),
            ("order", &[]),
            ("total", &["order"]),
        ]);
        assert_eq!(got, Ok(vec!["order", "total", "label"]));
    }

    #[test]
    fn properties_nothing_orders_fill_in_key_order() {
        let got = order(&[("c", &[]), ("a", &["z"]), ("b", &[]), ("z", &[])]);
        assert_eq!(got, Ok(vec!["b", "c", "z", "a"]));
    }

    #[test]
    fn a_cycle_fails_naming_its_properties() {
        let got = order(&[
            ("a", &[]),
            ("b", &["d"]),
            ("c", &["b"]),
            ("d", &["c"]),
            ("e", &["b"]),
        ]);
        let Err(Error::Invalid(message)) = got else {
            panic!("{got:?}")
        };
        assert!(message.ends_with(": b -> d -> c -> b"), "{message}");
        let Err(Error::Invalid(message)) = order(&[("a", &["a"])]) else {
            panic!()
        };
        assert!(message.ends_with(": a -> a"), "{message}");
    }
}
