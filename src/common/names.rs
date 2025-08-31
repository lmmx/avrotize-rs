use regex::Regex;
use serde_json::Value;

/// Convert a raw string into a valid Avro name.
///
/// Ensures the identifier starts with a letter or underscore,
/// replaces invalid characters with `_`, and prefixes leading digits.
pub fn avro_name(name: &str) -> String {
    let mut val = Regex::new(r"[^a-zA-Z0-9_]")
        .unwrap()
        .replace_all(name, "_")
        .to_string();
    if val.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
        val = format!("_{}", val);
    }
    if val.is_empty() || !val.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_') {
        val = format!("_{}", val);
    }
    val
}

/// Convert a name and return both normalized + altname if they differ.
pub fn avro_name_with_altname(name: &str) -> (String, Option<String>) {
    let normalized = avro_name(name);
    if normalized != name {
        (normalized, Some(name.to_string()))
    } else {
        (normalized, None)
    }
}

/// Convert an input string into a valid Avro namespace.
///
/// Replaces invalid chars with `_` but preserves dots as separators.
/// Prefixes with `_` if starting with a digit.
pub fn avro_namespace(name: &str) -> String {
    let mut val = Regex::new(r"[^a-zA-Z0-9_\.]")
        .unwrap()
        .replace_all(name, "_")
        .to_string();
    if val.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
        val = format!("_{}", val);
    }
    val
}


/// Convert string to PascalCase.
pub fn pascal(input: &str) -> String {
    if input.contains("::") {
        let mut parts = input.split("::");
        let head = parts.next().unwrap();
        return format!("{}::{}", head, parts.map(pascal).collect::<Vec<_>>().join("::"));
    }
    if input.contains('.') {
        return input
            .split('.')
            .map(pascal)
            .collect::<Vec<_>>()
            .join(".");
    }
    if input.is_empty() {
        return input.to_string();
    }

    let startswith_under = input.starts_with('_');
    let words: Vec<String>;

    if input.contains('_') {
        words = input.split('_').map(|w| w.to_string()).collect();
    } else if input.chars().next().unwrap().is_uppercase() {
        let re = Regex::new(r"[A-Z][a-z0-9_]*\.?").unwrap();
        words = re.find_iter(input).map(|m| m.as_str().to_string()).collect();
    } else {
        let re = Regex::new(r"[a-z0-9]+\.?|[A-Z][a-z0-9_]*\.?").unwrap();
        words = re.find_iter(input).map(|m| m.as_str().to_string()).collect();
    }

    let mut result = words.into_iter().map(|w| capitalize(&w)).collect::<String>();
    if startswith_under {
        result = format!("_{}", result);
    }
    result
}

/// Convert string to camelCase.
pub fn camel(input: &str) -> String {
    if input.contains("::") {
        let mut parts = input.split("::");
        let head = parts.next().unwrap();
        return format!("{}::{}", head, parts.map(camel).collect::<Vec<_>>().join("::"));
    }
    if input.contains('.') {
        return input
            .split('.')
            .map(camel)
            .collect::<Vec<_>>()
            .join(".");
    }
    if input.is_empty() {
        return input.to_string();
    }

    let words: Vec<String>;
    if input.contains('_') {
        words = input.split('_').map(|w| w.to_string()).collect();
    } else if input.chars().next().unwrap().is_uppercase() {
        let re = Regex::new(r"[A-Z][a-z0-9_]*\.?").unwrap();
        words = re.find_iter(input).map(|m| m.as_str().to_string()).collect();
    } else {
        let re = Regex::new(r"[a-z0-9]+\.?|[A-Z][a-z0-9_]*\.?").unwrap();
        words = re.find_iter(input).map(|m| m.as_str().to_string()).collect();
    }

    let mut iter = words.into_iter();
    let first = iter.next().unwrap_or_default().to_lowercase();
    let rest = iter.map(|w| capitalize(&w)).collect::<String>();

    format!("{}{}", first, rest)
}

/// Convert string to snake_case.
pub fn snake(input: &str) -> String {
    if input.contains("::") {
        let mut parts = input.split("::");
        let head = parts.next().unwrap();
        return format!("{}::{}", head, parts.map(snake).collect::<Vec<_>>().join("::"));
    }
    if input.contains('.') {
        return input
            .split('.')
            .map(snake)
            .collect::<Vec<_>>()
            .join(".");
    }
    if input.is_empty() {
        return input.to_string();
    }

    let words: Vec<String>;
    if input.contains('_') {
        words = input.split('_').map(|w| w.to_string()).collect();
    } else if input.chars().next().unwrap().is_uppercase() {
        let re = Regex::new(r"[A-Z][a-z0-9_]*\.?").unwrap();
        words = re.find_iter(input).map(|m| m.as_str().to_string()).collect();
    } else {
        let re = Regex::new(r"[a-z0-9]+\.?|[A-Z][a-z0-9_]*\.?").unwrap();
        words = re.find_iter(input).map(|m| m.as_str().to_string()).collect();
    }

    words.into_iter().map(|w| w.to_lowercase()).collect::<Vec<_>>().join("_")
}

/// Capitalize first letter
fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

/// Build full name of an Avro schema from `name` and `namespace`.
pub fn fullname(avro_schema: &Value, parent_namespace: &str) -> String {
    if let Some(s) = avro_schema.as_str() {
        if !s.contains('.') && !parent_namespace.is_empty() {
            return format!("{}.{}", parent_namespace, s);
        }
        return s.to_string();
    }
    let obj = avro_schema.as_object().unwrap();
    let name = obj.get("name").and_then(|n| n.as_str()).unwrap_or("");
    let namespace = obj
        .get("namespace")
        .and_then(|n| n.as_str())
        .unwrap_or(parent_namespace);
    if namespace.is_empty() {
        name.to_string()
    } else {
        format!("{}.{}", namespace, name)
    }
}

/// Get an alternate name for a schema object.
pub fn altname(schema_obj: &Value, purpose: &str) -> String {
    if let Some(altnames) = schema_obj.get("altnames") {
        if let Some(purpose_val) = altnames.get(purpose) {
            if let Some(s) = purpose_val.as_str() {
                return s.to_string();
            }
        }
    }
    schema_obj
        .get("name")
        .and_then(|n| n.as_str())
        .unwrap_or("")
        .to_string()
}

/// Get the longest namespace prefix among all namespaces.
pub fn get_longest_namespace_prefix(namespaces: &[String]) -> String {
    if namespaces.is_empty() {
        return String::new();
    }
    let mut prefix = namespaces[0].clone();
    for ns in namespaces.iter().skip(1) {
        let mut i = 0;
        while i < prefix.len() && i < ns.len() && prefix.as_bytes()[i] == ns.as_bytes()[i] {
            i += 1;
        }
        prefix.truncate(i);
    }
    prefix.trim_end_matches('.').to_string()
}

/// Parse generic arguments from a type string like `List[Map[String]]`.
pub fn get_typing_args_from_string(type_str: &str) -> Vec<String> {
    let re = Regex::new(r"([\w\.]+)\[(.+)\]").unwrap();
    if let Some(caps) = re.captures(type_str) {
        let args_str = caps.get(2).unwrap().as_str();
        let mut args = Vec::new();
        let mut depth = 0;
        let mut current = String::new();
        for ch in args_str.chars() {
            if ch == ',' && depth == 0 {
                args.push(current.trim().to_string());
                current.clear();
            } else {
                if ch == '[' {
                    depth += 1;
                } else if ch == ']' {
                    depth -= 1;
                }
                current.push(ch);
            }
        }
        if !current.is_empty() {
            args.push(current.trim().to_string());
        }
        args
    } else {
        Vec::new()
    }
}