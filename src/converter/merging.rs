use serde_json::Value;
use crate::converter::unions::flatten_union;

/// Merge multiple JSON Schemas into one.
///
/// This function combines object properties, required fields,
/// enums, and other metadata. It is lossy if conflicting
/// schema aspects overlap but differ.
pub fn merge_json_schemas(json_schemas: &[Value], intersect: bool) -> Value {
    fn merge_structures(schema1: &Value, schema2: &Value) -> Value {
        if let (Some(t1), Some(t2)) = (schema1.get("type"), schema2.get("type")) {
            if t1 != t2 {
                return Value::Array(vec![schema1.clone(), schema2.clone()]);
            }
        }

        let mut merged = schema1.clone();

        if let Some(obj2) = schema2.as_object() {
            let obj1 = merged.as_object_mut().unwrap();

            for (key, val2) in obj2 {
                match obj1.get_mut(key) {
                    None => {
                        obj1.insert(key.clone(), val2.clone());
                    }
                    Some(val1) => {
                        if val1.is_object() && val2.is_object() {
                            *val1 = merge_structures(val1, val2);
                        } else if val1.is_array() && val2.is_array() {
                            let mut arr = val1.as_array().unwrap().clone();
                            for item in val2.as_array().unwrap() {
                                if !arr.contains(item) {
                                    arr.push(item.clone());
                                }
                            }
                            *val1 = Value::Array(arr);
                        } else if val1 != val2 {
                            // conflict: put both into an array
                            *val1 = Value::Array(vec![val1.clone(), val2.clone()]);
                        }
                    }
                }
            }
        }

        merged
    }

    let mut merged: Value = Value::Object(serde_json::Map::new());

    for schema in json_schemas {
        if !schema.is_object() {
            continue;
        }

        if merged.get("type").is_none() || schema.get("type").is_none() {
            merged = merge_structures(&merged, schema);
        } else {
            if let (Some(t1), Some(t2)) = (merged.get("type"), schema.get("type")) {
                if t1 != t2 {
                    // multiple types → make union
                    let mut arr = if t1.is_array() {
                        t1.as_array().unwrap().clone()
                    } else {
                        vec![t1.clone()]
                    };
                    if !arr.contains(t2) {
                        arr.push(t2.clone());
                    }
                    let obj = merged.as_object_mut().unwrap();
                    obj.insert("type".to_string(), Value::Array(arr));
                }
            }
            merged = merge_structures(&merged, schema);
        }

        // handle required specially
        if let Some(req) = schema.get("required").and_then(|r| r.as_array()) {
            let req: Vec<String> = req.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect();
            let merged_req = merged.as_object_mut().unwrap().entry("required").or_insert_with(|| Value::Array(vec![]));
            if let Some(arr) = merged_req.as_array_mut() {
                for r in req {
                    if !arr.iter().any(|v| v.as_str() == Some(&r)) {
                        arr.push(Value::String(r));
                    }
                }
            }
        }
    }

    if intersect {
        if let Some(arr) = merged.get_mut("required").and_then(|r| r.as_array_mut()) {
            let mut set: Option<Vec<String>> = None;
            for schema in json_schemas {
                if let Some(req) = schema.get("required").and_then(|r| r.as_array()) {
                    let current: Vec<String> = req.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect();
                    set = Some(if let Some(prev) = set {
                        prev.into_iter().filter(|x| current.contains(x)).collect()
                    } else {
                        current
                    });
                }
            }
            if let Some(set) = set {
                *arr = set.into_iter().map(Value::String).collect();
            }
        }
    }

    merged
}

/// Merge multiple Avro type schemas into one.
///
/// This function handles:
/// - Deduplication of types,
/// - Merging of record fields,
/// - Combining union types,
/// - Propagating dependencies when present.
pub fn merge_avro_schemas(
    schemas: &[Value],
    avro_schemas: &[Value],
    type_name: Option<&str>,
    deps: &mut Vec<String>,
) -> Value {
    if schemas.len() == 1 {
        return schemas[0].clone();
    }

    let mut merged_schema = serde_json::Map::new();

    if let Some(name) = type_name {
        merged_schema.insert("name".to_string(), Value::String(name.to_string()));
    }

    for schema in schemas {
        if schema.is_null() || (schema.is_array() && schema.as_array().unwrap().is_empty()) {
            continue;
        }

        if let Some(obj) = schema.as_object() {
            // Merge dependencies
            if let Some(dependencies) = obj.get("dependencies").and_then(|d| d.as_array()) {
                for dep in dependencies {
                    if let Some(dep_str) = dep.as_str() {
                        deps.push(dep_str.to_string());
                    }
                }
            }

            for (key, value) in obj {
                match merged_schema.get_mut(key) {
                    Some(existing) => {
                        if existing != value {
                            // Merge into a union if conflict
                            let new_union = flatten_union(&[existing.clone(), value.clone()], avro_schemas);
                            *existing = Value::Array(new_union);
                        }
                    }
                    None => {
                        merged_schema.insert(key.clone(), value.clone());
                    }
                }
            }
        } else if let Some(s) = schema.as_str() {
            merged_schema.insert("type".to_string(), Value::String(s.to_string()));
        }
    }

    Value::Object(merged_schema)
}