use serde_json::Value;
use std::collections::HashMap;

/// Recursively search an Avro schema (serde_json::Value) for the first node matching `test`.
pub fn find_schema_node<F>(
    test: &F,
    avro_schema: &Value,
    recursion_stack: &mut Vec<*const Value>,
) -> Option<Value>
where
    F: Fn(&Value) -> bool,
{
    let ptr: *const Value = avro_schema as *const Value;

    if recursion_stack.contains(&ptr) {
        panic!("Cyclical reference detected in schema");
    }
    if recursion_stack.len() > 50 {
        panic!("Maximum recursion depth 50 exceeded in schema");
    }

    recursion_stack.push(ptr);

    let result = if avro_schema.is_object() {
        if test(avro_schema) {
            Some(avro_schema.clone())
        } else {
            for v in avro_schema.as_object().unwrap().values() {
                if v.is_object() || v.is_array() {
                    if let Some(found) = find_schema_node(test, v, recursion_stack) {
                        recursion_stack.pop();
                        return Some(found);
                    }
                }
            }
            None
        }
    } else if avro_schema.is_array() {
        for item in avro_schema.as_array().unwrap() {
            if item.is_object() || item.is_array() {
                if let Some(found) = find_schema_node(test, item, recursion_stack) {
                    recursion_stack.pop();
                    return Some(found);
                }
            }
        }
        None
    } else {
        None
    };

    recursion_stack.pop();
    result
}

/// Replace the first schema node matching `test` with `replacement`.
pub fn set_schema_node<F>(test: &F, replacement: &Value, avro_schema: &mut Value)
where
    F: Fn(&Value) -> bool,
{
    if avro_schema.is_object() {
        if test(avro_schema) {
            *avro_schema = replacement.clone();
            return;
        }
        if let Some(obj) = avro_schema.as_object_mut() {
            for v in obj.values_mut() {
                if v.is_object() || v.is_array() {
                    set_schema_node(test, replacement, v);
                }
            }
        }
    } else if avro_schema.is_array() {
        if let Some(arr) = avro_schema.as_array_mut() {
            for v in arr {
                set_schema_node(test, replacement, v);
            }
        }
    }
}

/// Collect all namespaces in a schema.
pub fn collect_namespaces(schema: &Value, parent_namespace: &str) -> Vec<String> {
    let mut namespaces = Vec::new();

    if let Some(obj) = schema.as_object() {
        let namespace = obj
            .get("namespace")
            .and_then(|n| n.as_str())
            .unwrap_or(parent_namespace)
            .to_string();
        if !namespace.is_empty() {
            namespaces.push(namespace.clone());
        }

        if let Some(fields) = obj.get("fields").and_then(|f| f.as_array()) {
            for field in fields {
                if let Some(field_obj) = field.as_object() {
                    if let Some(field_type) = field_obj.get("type") {
                        if field_type.is_object() {
                            namespaces.extend(collect_namespaces(field_type, &namespace));
                        }
                        namespaces.extend(collect_namespaces(field, &namespace));
                    }
                }
            }
        }
        if let Some(items) = obj.get("items") {
            if items.is_object() {
                namespaces.extend(collect_namespaces(items, &namespace));
            }
        }
        if let Some(values) = obj.get("values") {
            if values.is_object() {
                namespaces.extend(collect_namespaces(values, &namespace));
            }
        }
    } else if let Some(arr) = schema.as_array() {
        for item in arr {
            namespaces.extend(collect_namespaces(item, parent_namespace));
        }
    }

    namespaces
}

/// Build a flat dictionary of all named types in the schema.
pub fn build_flat_type_dict(avro_schema: &Value) -> HashMap<String, Value> {
    let mut type_dict = HashMap::new();

    fn add_to_dict(schema: &Value, namespace: &str, type_dict: &mut HashMap<String, Value>) {
        if let Some(obj) = schema.as_object() {
            let schema_type = obj.get("type").and_then(|v| v.as_str());
            let name = obj.get("name").and_then(|v| v.as_str());
            let namespace = obj
                .get("namespace")
                .and_then(|v| v.as_str())
                .unwrap_or(namespace);

            if let (Some(schema_type), Some(name)) = (schema_type, name) {
                if ["record", "enum", "fixed"].contains(&schema_type) {
                    let qualified = if namespace.is_empty() {
                        name.to_string()
                    } else {
                        format!("{}.{}", namespace, name)
                    };
                    type_dict.insert(qualified, schema.clone());
                }
            }

            if schema_type == Some("record") {
                if let Some(fields) = obj.get("fields").and_then(|f| f.as_array()) {
                    for field in fields {
                        if let Some(field_type) = field.get("type") {
                            add_to_dict(field_type, namespace, type_dict);
                        }
                    }
                }
            } else if schema_type == Some("array") {
                if let Some(items) = obj.get("items") {
                    add_to_dict(items, namespace, type_dict);
                }
            } else if schema_type == Some("map") {
                if let Some(values) = obj.get("values") {
                    add_to_dict(values, namespace, type_dict);
                }
            }
        } else if let Some(arr) = schema.as_array() {
            for item in arr {
                add_to_dict(item, namespace, type_dict);
            }
        }
    }

    add_to_dict(avro_schema, "", &mut type_dict);
    type_dict
}
