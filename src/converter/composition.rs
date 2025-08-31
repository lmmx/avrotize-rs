use serde_json::Value;

/// Merge two JSON schema objects recursively.
///
/// This is used inside `allOf`, `oneOf`, and `anyOf` handling.
fn merge_structures(schema1: &Value, schema2: &Value) -> Value {
    if !schema1.is_object() || !schema2.is_object() {
        return schema1.clone();
    }

    let mut merged = schema1.as_object().unwrap().clone();

    for (key, val2) in schema2.as_object().unwrap() {
        match merged.get_mut(key) {
            Some(val1) => {
                if val1 == val2 {
                    continue;
                }
                if val1.is_object() && val2.is_object() {
                    *val1 = merge_structures(val1, val2);
                } else if val1.is_array() && val2.is_array() {
                    let mut arr = val1.as_array().unwrap().clone();
                    for v in val2.as_array().unwrap() {
                        if !arr.contains(v) {
                            arr.push(v.clone());
                        }
                    }
                    *val1 = Value::Array(arr);
                } else if val1.is_array() {
                    if !val1.as_array().unwrap().contains(val2) {
                        let mut arr = val1.as_array().unwrap().clone();
                        arr.push(val2.clone());
                        *val1 = Value::Array(arr);
                    }
                } else {
                    *val1 = Value::Array(vec![val1.clone(), val2.clone()]);
                }
            }
            None => {
                merged.insert(key.clone(), val2.clone());
            }
        }
    }

    Value::Object(merged)
}

/// Merge multiple JSON schemas into one.
///
/// - `allOf` → merge all subtypes into a single schema.
/// - `oneOf` → keep alternatives as a union of schemas.
/// - `anyOf` → merge as a flexible union, approximated by Avro union rules.
pub fn merge_json_schemas(json_schemas: &[Value], intersect: bool) -> Value {
    let mut merged: serde_json::Map<String, Value> = serde_json::Map::new();

    for schema in json_schemas {
        if !schema.is_object() {
            continue;
        }
        for (key, value) in schema.as_object().unwrap() {
            match merged.get_mut(key) {
                Some(existing) => {
                    if existing == value {
                        continue;
                    }
                    if existing.is_object() && value.is_object() {
                        *existing = merge_structures(existing, value);
                    } else if existing.is_array() && value.is_array() {
                        let mut arr = existing.as_array().unwrap().clone();
                        for v in value.as_array().unwrap() {
                            if !arr.contains(v) {
                                arr.push(v.clone());
                            }
                        }
                        *existing = Value::Array(arr);
                    } else if existing.is_array() {
                        if !existing.as_array().unwrap().contains(value) {
                            let mut arr = existing.as_array().unwrap().clone();
                            arr.push(value.clone());
                            *existing = Value::Array(arr);
                        }
                    } else {
                        *existing = Value::Array(vec![existing.clone(), value.clone()]);
                    }
                }
                None => {
                    merged.insert(key.clone(), value.clone());
                }
            }
        }
    }

    if intersect {
        if let Some(Value::Array(reqs)) = merged.get_mut("required") {
            let mut intersection = reqs.clone();
            for schema in json_schemas {
                if let Some(Value::Array(req2)) = schema.get("required") {
                    intersection.retain(|r| req2.contains(r));
                }
            }
            *reqs = intersection;
        }
    }

    Value::Object(merged)
}

/// Handle `allOf`, `oneOf`, or `anyOf` composition inside a JSON schema.
///
/// Returns a list of merged schema alternatives.
pub fn expand_composition(base: &Value, keyword: &str) -> Vec<Value> {
    if !base.is_object() {
        return vec![base.clone()];
    }
    let obj = base.as_object().unwrap();

    if let Some(Value::Array(subschemas)) = obj.get(keyword) {
        match keyword {
            "allOf" => {
                let mut schemas = vec![base.clone()];
                schemas.extend(subschemas.clone());
                vec![merge_json_schemas(&schemas, false)]
            }
            "oneOf" | "anyOf" => {
                let mut results = Vec::new();
                for s in subschemas {
                    let merged = merge_json_schemas(&[base.clone(), s.clone()], keyword == "oneOf");
                    results.push(merged);
                }
                results
            }
            _ => vec![base.clone()],
        }
    } else {
        vec![base.clone()]
    }
}