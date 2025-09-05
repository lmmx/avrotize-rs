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

/// Convert a string into a valid Avro name by replacing
/// invalid characters with `_` and ensuring it starts with
/// a letter or underscore.
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

/// Normalize a string to a valid Avro name and return both
/// the normalized name and the original name if they differ.
pub fn avro_name_with_altname(name: &str) -> (String, Option<String>) {
    let normalized = avro_name(name);
    if normalized != name {
        (normalized, Some(name.to_string()))
    } else {
        (normalized, None)
    }
}

/// Convert a string into a valid Avro namespace, allowing dots (`.`).
/// Invalid characters are replaced with `_`.
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
