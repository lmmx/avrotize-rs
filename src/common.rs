use regex::Regex;

pub mod generic;
pub mod hash;
pub mod inline;
pub mod names;
pub mod traversal;

pub use generic::*;
pub use hash::*;
pub use inline::*;
pub use names::*;
pub use traversal::*;

/// Convert a string into a valid Avro name.
pub fn avro_name(name: &str) -> String {
    let mut val = Regex::new(r"[^a-zA-Z0-9_]")
        .unwrap()
        .replace_all(name, "_")
        .to_string();

    if val
        .chars()
        .next()
        .map(|c| c.is_ascii_digit())
        .unwrap_or(false)
    {
        val = format!("_{}", val);
    }

    if val.is_empty()
        || !val
            .chars()
            .next()
            .map(|c| c.is_ascii_alphabetic() || c == '_')
            .unwrap_or(false)
    {
        val = format!("_{}", val);
    }

    val
}

/// Return normalized Avro name and alternate name if different.
pub fn avro_name_with_altname(name: &str) -> (String, Option<String>) {
    let normalized = avro_name(name);
    if normalized != name {
        (normalized, Some(name.to_string()))
    } else {
        (normalized, None)
    }
}

/// Convert string to Avro namespace (allows dots).
pub fn avro_namespace(name: &str) -> String {
    let mut val = Regex::new(r"[^a-zA-Z0-9_\.]")
        .unwrap()
        .replace_all(name, "_")
        .to_string();

    if val
        .chars()
        .next()
        .map(|c| c.is_ascii_digit())
        .unwrap_or(false)
    {
        val = format!("_{}", val);
    }
    val
}
