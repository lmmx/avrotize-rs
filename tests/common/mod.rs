//! An Avro schema checker for the integration tests.
//!
//! It enforces the parts of the Avro specification that the converter has broken:
//! a full name is defined once, references resolve, unions hold no unions and no two
//! members of the same type, field names and enum symbols are unique. `fits` then
//! says whether a JSON value can be written with a schema without loss.
#![allow(dead_code)] // each test crate uses a different part of this module

use serde_json::Value;
use std::collections::{HashMap, HashSet};

/// Every object in `avro` that is a type, skipping `const` and `default` data.
pub fn types(avro: &Value) -> Vec<&Value> {
    let mut out = Vec::new();
    match avro {
        Value::Object(obj) => {
            if obj.contains_key("type") {
                out.push(avro);
            }
            for (k, v) in obj {
                if k != "const" && k != "default" {
                    out.extend(types(v));
                }
            }
        }
        Value::Array(arr) => arr.iter().for_each(|v| out.extend(types(v))),
        _ => {}
    }
    out
}

pub fn assert_no_bookkeeping(avro: &Value) {
    for t in types(avro) {
        for key in ["dependencies", "unmerged_types"] {
            assert!(t.get(key).is_none(), "{key} leaked into {t}");
        }
    }
}

/// The named types `avro` defines, by full name, checked the way an Avro parser
/// checks them: no full name is defined twice, and every reference is defined.
pub fn named_types(avro: &Value) -> HashMap<String, &Value> {
    const PRIMITIVES: [&str; 8] = [
        "null", "boolean", "int", "long", "float", "double", "bytes", "string",
    ];
    let mut defined = HashMap::new();
    let mut referenced = Vec::new();
    for t in types(avro) {
        if let (Some("record" | "enum" | "fixed"), Some(name)) =
            (t["type"].as_str(), t["name"].as_str())
        {
            let full = match t["namespace"].as_str() {
                Some(ns) if !ns.is_empty() => format!("{ns}.{name}"),
                _ => name.to_string(),
            };
            assert!(
                defined.insert(full.clone(), t).is_none(),
                "{full} is defined twice"
            );
        }
        for key in ["type", "items", "values"] {
            let refs = match &t[key] {
                Value::Array(union) => union.iter().collect(),
                other => vec![other],
            };
            referenced.extend(refs.into_iter().filter_map(Value::as_str));
        }
    }
    for name in referenced {
        let is_type_keyword = matches!(name, "record" | "enum" | "fixed" | "array" | "map");
        assert!(
            is_type_keyword || PRIMITIVES.contains(&name) || defined.contains_key(name),
            "{name} is referenced but not defined"
        );
    }
    defined
}

/// Whether the JSON `datum` can be written with the Avro `schema`.
pub fn fits(schema: &Value, datum: &Value, names: &HashMap<String, &Value>) -> bool {
    match schema {
        Value::String(s) => match s.as_str() {
            "null" => datum.is_null(),
            "boolean" => datum.is_boolean(),
            "int" | "long" => datum.is_i64() || datum.is_u64(),
            "float" | "double" => datum.is_number(),
            "string" | "bytes" => datum.is_string(),
            name => fits(names[name], datum, names),
        },
        Value::Array(union) => branch(union, datum, names).is_some(),
        Value::Object(obj) => match obj["type"].as_str() {
            Some("record") => datum.as_object().is_some_and(|d| {
                let fields = obj["fields"].as_array().unwrap();
                d.keys().all(|k| fields.iter().any(|f| f["name"] == *k))
                    && fields.iter().all(|f| {
                        let value = d.get(f["name"].as_str().unwrap());
                        fits(&f["type"], value.unwrap_or(&Value::Null), names)
                    })
            }),
            Some("array") => datum
                .as_array()
                .is_some_and(|items| items.iter().all(|i| fits(&obj["items"], i, names))),
            Some("map") => datum
                .as_object()
                .is_some_and(|m| m.values().all(|v| fits(&obj["values"], v, names))),
            Some("enum") => datum
                .as_str()
                .is_some_and(|s| obj["symbols"].as_array().unwrap().iter().any(|x| x == s)),
            _ => fits(&obj["type"], datum, names),
        },
        _ => false,
    }
}

/// The first branch of `union` that `datum` can be written with.
pub fn branch(union: &[Value], datum: &Value, names: &HashMap<String, &Value>) -> Option<usize> {
    union.iter().position(|b| fits(b, datum, names))
}

/// The identity of a union member for the "no two of the same type" rule: named
/// types by full name (references included), everything else by its type.
fn union_member_key(member: &Value, names: &HashMap<String, &Value>) -> String {
    match member {
        Value::String(s) if names.contains_key(s.as_str()) => format!("named:{s}"),
        Value::String(s) => s.clone(),
        Value::Object(obj) => match obj.get("type") {
            Some(Value::String(t)) if matches!(t.as_str(), "record" | "enum" | "fixed") => {
                let name = obj["name"].as_str().unwrap_or("");
                match obj.get("namespace").and_then(Value::as_str) {
                    Some(ns) if !ns.is_empty() => format!("named:{ns}.{name}"),
                    _ => format!("named:{name}"),
                }
            }
            Some(Value::String(t)) => t.clone(), // array, map, or a primitive with a logical type
            Some(other) => format!("{other}"),
            None => "?".into(),
        },
        Value::Array(_) => "union".into(),
        other => format!("{other}"),
    }
}

/// Assert that `avro` is a well-formed Avro schema, as strict parsers require.
pub fn assert_well_formed(avro: &Value) {
    let names = named_types(avro);
    let check_union = |union: &Vec<Value>, at: &Value| {
        let mut seen = HashSet::new();
        for member in union {
            assert!(
                !member.is_array(),
                "a union directly contains a union in {at}"
            );
            let key = union_member_key(member, &names);
            assert!(
                seen.insert(key.clone()),
                "a union holds {key} twice in {at}"
            );
        }
    };
    if let Value::Array(union) = avro {
        if !union.is_empty() {
            check_union(union, avro);
        }
    }
    for t in types(avro) {
        for key in ["type", "items", "values"] {
            if let Value::Array(union) = &t[key] {
                assert!(!union.is_empty(), "an empty union in {t}");
                check_union(union, t);
            }
        }
        match t["type"].as_str() {
            Some("record") => {
                let mut fields = HashSet::new();
                for f in t["fields"].as_array().expect("a record has fields") {
                    let name = f["name"].as_str().expect("a field has a name");
                    assert!(fields.insert(name), "field {name} appears twice in {t}");
                }
            }
            Some("enum") => {
                let symbols = t["symbols"].as_array().expect("an enum has symbols");
                assert!(!symbols.is_empty(), "an enum without symbols: {t}");
                let unique: HashSet<_> = symbols.iter().collect();
                assert_eq!(unique.len(), symbols.len(), "repeated enum symbols in {t}");
            }
            Some("fixed") => assert!(t.get("name").is_some(), "a fixed type without a name: {t}"),
            _ => {}
        }
    }
}
