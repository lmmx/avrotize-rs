use serde_json::Value;
use std::collections::HashMap;
use std::collections::HashSet;

/// Evict all tracked references in the Avro schema.
/// If a record/enum/fixed type has been seen, replace future uses with its qualified name.
pub fn evict_tracked_references(
    avro_schema: &Value,
    parent_namespace: &str,
    tracker: &mut HashSet<String>,
) -> Value {
    if let Some(obj) = avro_schema.as_object() {
        if let Some(t) = obj.get("type").and_then(|t| t.as_str()) {
            if ["record", "enum", "fixed"].contains(&t) {
                let namespace = obj
                    .get("namespace")
                    .and_then(|n| n.as_str())
                    .unwrap_or(parent_namespace);
                let qualified_name = if namespace.is_empty() {
                    obj.get("name").unwrap().as_str().unwrap().to_string()
                } else {
                    format!("{}.{}", namespace, obj.get("name").unwrap().as_str().unwrap())
                };
                if !tracker.contains(&qualified_name) {
                    let mut new_obj = obj.clone();
                    if let Some(fields) = new_obj.get_mut("fields").and_then(|f| f.as_array_mut()) {
                        let mut replaced = Vec::new();
                        for field in fields.iter() {
                            let new_type = evict_tracked_references(field.get("type").unwrap(), namespace, tracker);
                            let mut field_clone = field.clone();
                            if let Some(map) = field_clone.as_object_mut() {
                                map.insert("type".to_string(), new_type);
                            }
                            replaced.push(field_clone);
                        }
                        *fields = replaced;
                    }
                    return Value::Object(new_obj);
                } else {
                    return Value::String(qualified_name);
                }
            } else if t == "array" {
                if let Some(items) = obj.get("items") {
                    let mut new_obj = obj.clone();
                    new_obj.insert("items".to_string(), evict_tracked_references(items, parent_namespace, tracker));
                    return Value::Object(new_obj);
                }
            } else if t == "map" {
                if let Some(values) = obj.get("values") {
                    let mut new_obj = obj.clone();
                    new_obj.insert("values".to_string(), evict_tracked_references(values, parent_namespace, tracker));
                    return Value::Object(new_obj);
                }
            }
        }
    } else if let Some(arr) = avro_schema.as_array() {
        return Value::Array(
            arr.iter()
                .map(|item| evict_tracked_references(item, parent_namespace, tracker))
                .collect(),
        );
    }
    avro_schema.clone()
}

/// Inline the first reference to a type in the Avro schema.
pub fn inline_avro_references(
    avro_schema: &Value,
    type_dict: &HashMap<String, Value>,
    current_namespace: &str,
    tracker: &mut HashSet<String>,
    defined_types: &mut HashSet<String>,
) -> Value {
    if let Some(obj) = avro_schema.as_object() {
        let mut new_obj = obj.clone();

        if let Some(t) = obj.get("type").and_then(|t| t.as_str()) {
            if ["record", "enum", "fixed"].contains(&t) {
                let namespace = obj
                    .get("namespace")
                    .and_then(|n| n.as_str())
                    .unwrap_or(current_namespace);
                let qualified = format!(
                    "{}{}{}",
                    if namespace.is_empty() { "" } else { namespace },
                    if namespace.is_empty() { "" } else { "." },
                    obj.get("name").unwrap().as_str().unwrap()
                );
                defined_types.insert(qualified.clone());
            }
        }

        if obj.get("type").map(|t| t.as_str().unwrap_or("")) == Some("record") {
            let namespace = obj
                .get("namespace")
                .and_then(|n| n.as_str())
                .unwrap_or(current_namespace);
            let qualified = format!(
                "{}{}{}",
                if namespace.is_empty() { "" } else { namespace },
                if namespace.is_empty() { "" } else { "." },
                obj.get("name").unwrap().as_str().unwrap()
            );
            if tracker.contains(&qualified) {
                return Value::String(qualified);
            }
            tracker.insert(qualified);
            if let Some(fields) = obj.get("fields").and_then(|f| f.as_array()) {
                let new_fields: Vec<Value> = fields
                    .iter()
                    .map(|field| {
                        let mut field_clone = field.clone();
                        if let Some(ftype) = field.get("type") {
                            let inlined = inline_avro_references(
                                ftype,
                                type_dict,
                                namespace,
                                tracker,
                                defined_types,
                            );
                            if let Some(map) = field_clone.as_object_mut() {
                                map.insert("type".to_string(), inlined);
                            }
                        }
                        field_clone
                    })
                    .collect();
                new_obj.insert("fields".to_string(), Value::Array(new_fields));
            }
            return Value::Object(new_obj);
        }

        if obj.get("type").map(|t| t.as_str().unwrap_or("")) == Some("array") {
            if let Some(items) = obj.get("items") {
                new_obj.insert(
                    "items".to_string(),
                    inline_avro_references(items, type_dict, current_namespace, tracker, defined_types),
                );
            }
            return Value::Object(new_obj);
        }

        if obj.get("type").map(|t| t.as_str().unwrap_or("")) == Some("map") {
            if let Some(values) = obj.get("values") {
                new_obj.insert(
                    "values".to_string(),
                    inline_avro_references(values, type_dict, current_namespace, tracker, defined_types),
                );
            }
            return Value::Object(new_obj);
        }

        if let Some(inner) = obj.get("type") {
            let inlined = inline_avro_references(inner, type_dict, current_namespace, tracker, defined_types);
            new_obj.insert("type".to_string(), inlined);
            return Value::Object(new_obj);
        }

        Value::Object(new_obj)
    } else if let Some(arr) = avro_schema.as_array() {
        Value::Array(
            arr.iter()
                .map(|item| inline_avro_references(item, type_dict, current_namespace, tracker, defined_types))
                .collect(),
        )
    } else if let Some(type_str) = avro_schema.as_str() {
        if let Some(schema) = type_dict.get(type_str) {
            if !tracker.contains(type_str) && !defined_types.contains(type_str) {
                let mut schema_clone = schema.clone();
                if schema_clone.get("namespace").is_none() {
                    let parts: Vec<&str> = type_str.rsplitn(2, '.').collect();
                    if parts.len() == 2 {
                        schema_clone
                            .as_object_mut()
                            .unwrap()
                            .insert("namespace".to_string(), Value::String(parts[1].to_string()));
                    }
                }
                let inlined = inline_avro_references(&schema_clone, type_dict, current_namespace, tracker, defined_types);
                tracker.insert(type_str.to_string());
                return inlined;
            }
        }
        Value::String(type_str.to_string())
    } else {
        avro_schema.clone()
    }
}

/// Strip the first "doc" field found anywhere in the schema.
pub fn strip_first_doc(schema: &mut Value) -> bool {
    if let Some(obj) = schema.as_object_mut() {
        if obj.contains_key("doc") {
            obj.remove("doc");
            return true;
        }
        for v in obj.values_mut() {
            if strip_first_doc(v) {
                return true;
            }
        }
    } else if let Some(arr) = schema.as_array_mut() {
        for v in arr {
            if strip_first_doc(v) {
                return true;
            }
        }
    }
    false
}

/// Strip the alternate type from an Avro schema union (if present).
pub fn strip_alternate_type(avro_schema: &mut Vec<Value>) {
    if let Some(_original) = avro_schema.iter().find(|t| t.is_object() && !t.get("alternateof").is_some()) {
        if let Some(alternate) = avro_schema.iter().find(|t| t.is_object() && t.get("alternateof").is_some()) {
            let idx = avro_schema.iter().position(|x| x == alternate).unwrap();
            avro_schema.remove(idx);
        }
    }
}