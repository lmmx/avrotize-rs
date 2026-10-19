use serde_json::Value;
use std::collections::{HashMap, HashSet};

use crate::common::traversal::{find_schema_node, set_schema_node};
use crate::converter::analysis::is_standalone_avro_type;
use crate::converter::merging::merge_avro_schemas;
use crate::converter::utils::lift_dependencies_from_type;

/// Finalize an Avro type after construction.
///
/// If the type is a dict whose `"type"` is not a complex type,
/// just return the inner type and push dependencies upward.
pub fn post_check_avro_type(dependencies: &mut Vec<String>, avro_type: Value) -> Value {
    if let Some(obj) = avro_type.as_object() {
        if let Some(Value::String(t)) = obj.get("type") {
            if !matches!(t.as_str(), "array" | "map" | "record" | "enum" | "fixed") {
                let mut new_deps = Vec::new();
                let mut copy = avro_type.clone();
                lift_dependencies_from_type(&mut copy, &mut new_deps);
                dependencies.extend(new_deps);
                return Value::String(t.clone());
            }
        }
    }
    avro_type
}

/// Register a type in the Avro schema list.
///
/// Ensures no duplicate types by name + namespace.
/// Returns true if the type was added.
pub fn register_type(avro_schema: &mut Vec<Value>, avro_type: Value) -> bool {
    let name = avro_type.get("name").and_then(|n| n.as_str()).unwrap_or("");
    let namespace = avro_type
        .get("namespace")
        .and_then(|n| n.as_str())
        .unwrap_or("");

    let exists = avro_schema.iter().any(|t| {
        t.get("name").and_then(|n| n.as_str()) == Some(name)
            && t.get("namespace").and_then(|n| n.as_str()) == Some(namespace)
    });

    if !exists && !avro_type.is_null() && is_standalone_avro_type(&avro_type) {
        avro_schema.push(avro_type);
        return true;
    }
    exists
}

/// Perform a second pass to resolve "unmerged_types" fields.
///
/// This reconciles placeholder union/anyOf types into merged Avro forms.
pub fn postprocess_schema(avro_schema: &mut [Value], types_with_unmerged: Vec<Value>) {
    for ref_type in types_with_unmerged {
        let name = ref_type.get("name").and_then(|n| n.as_str()).unwrap_or("");
        let namespace = ref_type
            .get("namespace")
            .and_then(|n| n.as_str())
            .unwrap_or("");

        // find matching type in the schema
        let mut recursion_stack = Vec::new();
        let found = find_schema_node(
            &|t: &Value| {
                t.get("name").and_then(|n| n.as_str()) == Some(name)
                    && t.get("namespace").and_then(|n| n.as_str()) == Some(namespace)
            },
            &Value::Array(avro_schema.to_vec()),
            &mut recursion_stack,
        );

        if let Some(found_type) = found {
            let unmerged = found_type
                .get("unmerged_types")
                .and_then(|u| u.as_array())
                .cloned()
                .unwrap_or_default();

            if !unmerged.is_empty() {
                let mut base = found_type.clone();
                if let Some(obj) = base.as_object_mut() {
                    obj.remove("unmerged_types");
                }

                let mut deps = Vec::new();
                lift_dependencies_from_type(&mut base, &mut deps);

                let mut mergeable = vec![base];
                mergeable.extend(unmerged);

                let merged = merge_avro_schemas(&mergeable, &[], Some(name), &mut deps);

                let _recursion_stack: Vec<*const Value> = Vec::new();
                set_schema_node(
                    &|t: &Value| {
                        t.get("name").and_then(|n| n.as_str()) == Some(name)
                            && t.get("namespace").and_then(|n| n.as_str()) == Some(namespace)
                    },
                    &merged,
                    &mut Value::Array(avro_schema.to_vec()),
                );
            }
        }
    }
}

/// Keys the converter keeps on types for its own use, which are not Avro attributes.
const BOOKKEEPING_KEYS: [&str; 2] = ["dependencies", "unmerged_types"];

/// The `(namespace, name)` of a named Avro type (record, enum or fixed).
fn named_type_key(avro_type: &Value) -> Option<(String, String)> {
    let obj = avro_type.as_object()?;
    let ty = obj.get("type")?.as_str()?;
    if !matches!(ty, "record" | "enum" | "fixed") {
        return None;
    }
    let name = obj.get("name")?.as_str()?;
    let namespace = obj.get("namespace").and_then(|n| n.as_str()).unwrap_or("");
    Some((namespace.to_string(), name.to_string()))
}

/// Give each differently shaped named type its own name.
///
/// Types are named after the property they came from, so two differently shaped
/// objects under the same key and namespace get the same full name, and dependency
/// resolution would turn the second into a reference to the first, losing its fields.
///
/// In document order, the first definition of a full name keeps it. A later identical
/// definition keeps it too (and is later emitted as a reference), while a later
/// different one takes the name of an earlier identical renamed one or else the first
/// free ordinal suffix (`r_1`, `r_2`, ...).
pub fn disambiguate_named_types(avro_schema: &mut [Value]) {
    fn collect_names(avro_type: &Value, taken: &mut HashSet<(String, String)>) {
        if let Some(key) = named_type_key(avro_type) {
            taken.insert(key);
        }
        match avro_type {
            Value::Object(obj) => obj.values().for_each(|v| collect_names(v, taken)),
            Value::Array(arr) => arr.iter().for_each(|v| collect_names(v, taken)),
            _ => {}
        }
    }

    /// Definitions seen so far, per original full name: (shape, name given).
    type Seen = HashMap<(String, String), Vec<(Value, String)>>;

    fn rename(avro_type: &mut Value, seen: &mut Seen, taken: &mut HashSet<(String, String)>) {
        if let Some((namespace, name)) = named_type_key(avro_type) {
            let mut shape = avro_type.clone();
            strip_bookkeeping(&mut shape);
            let defs = seen.entry((namespace.clone(), name.clone())).or_default();
            let given = match defs.iter().find(|(s, _)| *s == shape) {
                Some((_, given)) => given.clone(),
                None if defs.is_empty() => name.clone(),
                None => {
                    let fresh = (1..)
                        .map(|i| format!("{name}_{i}"))
                        .find(|n| !taken.contains(&(namespace.clone(), n.clone())))
                        .unwrap();
                    taken.insert((namespace.clone(), fresh.clone()));
                    fresh
                }
            };
            if !defs.iter().any(|(s, _)| *s == shape) {
                defs.push((shape, given.clone()));
            }
            if given != name {
                avro_type["name"] = Value::String(given);
            }
        }
        match avro_type {
            Value::Object(obj) => obj.values_mut().for_each(|v| rename(v, seen, taken)),
            Value::Array(arr) => arr.iter_mut().for_each(|v| rename(v, seen, taken)),
            _ => {}
        }
    }

    let mut taken = HashSet::new();
    avro_schema
        .iter()
        .for_each(|t| collect_names(t, &mut taken));
    let mut seen = Seen::new();
    avro_schema
        .iter_mut()
        .for_each(|t| rename(t, &mut seen, &mut taken));
}

/// Remove the converter's bookkeeping keys from every type in an Avro schema.
///
/// `const` and `default` values are data, so they are left as they are.
pub fn strip_bookkeeping(avro_type: &mut Value) {
    match avro_type {
        Value::Object(obj) => {
            if obj.contains_key("type") && BOOKKEEPING_KEYS.iter().any(|k| obj.contains_key(*k)) {
                *obj = std::mem::take(obj)
                    .into_iter()
                    .filter(|(k, _)| !BOOKKEEPING_KEYS.contains(&k.as_str()))
                    .collect();
            }
            for (k, v) in obj.iter_mut() {
                if k != "const" && k != "default" {
                    strip_bookkeeping(v);
                }
            }
        }
        Value::Array(arr) => arr.iter_mut().for_each(strip_bookkeeping),
        _ => {}
    }
}
