#[cfg_attr(feature = "trace", crustrace::omni)]
mod innermod {
    use crate::common::generic::generic_type;
    use crate::common::names::{avro_name, pascal};
    use crate::converter::analysis::{has_composition_keywords, has_enum_keyword, is_array_object};
    use crate::converter::merging::{merge_avro_schemas, merge_json_schemas};
    use crate::converter::structs::{
        create_array_type, create_avro_record, create_enum_type, create_map_type,
        create_wrapper_record,
    };
    use crate::converter::types::json_schema_primitive_to_avro_type;
    use crate::converter::utils::{merge_dependencies_into_parent, merge_description_into_doc};
    use serde_json::{json, Value};

    /// Handle `patternProperties` in a JSON Schema object.
    fn handle_pattern_properties(
        json_object: &Value,
        record_name: &str,
        namespace: &str,
        utility_namespace: &str,
        base_uri: &str,
        avro_schema: &mut Vec<Value>,
        record_stack: &mut Vec<String>,
        dependencies: &mut Vec<String>,
    ) -> Vec<Value> {
        let mut extension_types = Vec::new();

        if let Some(pattern_props) = json_object
            .get("patternProperties")
            .and_then(|pp| pp.as_object())
        {
            for (pattern, prop_schema) in pattern_props {
                let mut deps = Vec::new();
                let avro_type = json_type_to_avro_type(
                    prop_schema,
                    record_name,
                    pattern,
                    namespace,
                    utility_namespace,
                    &mut deps,
                    json_object,
                    base_uri,
                    avro_schema,
                    record_stack,
                    1,
                );
                extension_types.push(avro_type);
                dependencies.extend(deps);
            }
        }

        extension_types
    }

    /// Handle `additionalProperties` in a JSON Schema object.
    fn handle_additional_properties(
        json_object: &Value,
        record_name: &str,
        namespace: &str,
        utility_namespace: &str,
        base_uri: &str,
        avro_schema: &mut Vec<Value>,
        record_stack: &mut Vec<String>,
        dependencies: &mut Vec<String>,
    ) -> Option<Value> {
        if let Some(additional) = json_object.get("additionalProperties") {
            if additional.is_boolean() {
                if additional.as_bool().unwrap() {
                    // "additionalProperties": true -> generic map<string, any>
                    return Some(json!({
                        "type": "map",
                        "values": "string"
                    }));
                }
            } else if additional.is_object() {
                let mut deps = Vec::new();
                let avro_type = json_type_to_avro_type(
                    additional,
                    record_name,
                    &(record_name.to_string() + "_extensions"),
                    namespace,
                    utility_namespace,
                    &mut deps,
                    json_object,
                    base_uri,
                    avro_schema,
                    record_stack,
                    1,
                );
                dependencies.extend(deps);
                return Some(json!({
                    "type": "map",
                    "values": avro_type
                }));
            }
        }
        None
    }

    /// Convert a JSON schema object declaration to an Avro record.
    pub fn json_schema_object_to_avro_record(
        name: &str,
        json_object: &Value,
        namespace: &str,
        utility_namespace: &str,
        json_schema: &Value,
        base_uri: &str,
        avro_schema: &mut Vec<Value>,
        record_stack: &mut Vec<String>,
    ) -> Value {
        if json_object.as_object().map_or(false, |obj| obj.is_empty()) {
            return Value::Array(vec![]);
        }
        let mut dependencies: Vec<String> = Vec::new();

        if let Some(ref_str) = json_object.get("$ref").and_then(|r| r.as_str()) {
            if let Some(def_name) = ref_str.strip_prefix("#/$defs/") {
                // 👉 Just return the Avro type name that was registered by process_definition
                let fq_name = format!("{}.{}", namespace, def_name);
                return json!(fq_name);
            }

            if let Some(ptr) = ref_str.strip_prefix('#') {
                if let Some(resolved) = json_schema.pointer(ptr) {
                    return json_schema_object_to_avro_record(
                        name,
                        resolved,
                        namespace,
                        utility_namespace,
                        json_schema,
                        base_uri,
                        avro_schema,
                        record_stack,
                    );
                }
            }

            eprintln!(
                "WARN: external $ref resolution not yet implemented: {}",
                ref_str
            );
            return json!("string"); // placeholder
        }

        // Composition keywords: allOf, oneOf, anyOf
        if has_composition_keywords(json_object) {
            let t = json_type_to_avro_type(
                json_object,
                name,
                "",
                namespace,
                utility_namespace,
                &mut dependencies,
                json_schema,
                base_uri,
                avro_schema,
                record_stack,
                1,
            );

            let mut avro_type = if t.is_array() {
                create_wrapper_record(
                    &(name.to_string() + "_union"),
                    utility_namespace,
                    "options",
                    &[],
                    t,
                )
            } else if t.get("type").is_some() && t.get("type").unwrap() != "record" {
                create_wrapper_record(
                    &(name.to_string() + "_wrapper"),
                    utility_namespace,
                    "value",
                    &[],
                    t,
                )
            } else {
                t
            };
            // Merge dependencies from the wrapped inner type into the wrapper record itself
            if avro_type.get("fields").is_some() {
                // Move the whole fields array out
                let mut fields_val = avro_type["fields"].take();

                if let Some(fields) = fields_val.as_array_mut() {
                    if let Some(first_field) = fields.first_mut() {
                        if let Some(field_type) = first_field.get_mut("type") {
                            let mut inner = field_type.take();
                            merge_dependencies_into_parent(
                                &mut dependencies,
                                &mut inner,
                                &mut avro_type,
                            );
                            *field_type = inner;
                        }
                    }
                }

                // Put the fields array back
                avro_type["fields"] = fields_val;
            }
            merge_description_into_doc(json_object, &mut avro_type);
            return avro_type;
        }

        // Enum
        if has_enum_keyword(json_object) {
            if let Some(enum_vals) = json_object.get("enum").and_then(|v| v.as_array()) {
                let symbols: Vec<String> = enum_vals
                    .iter()
                    .filter_map(|v| v.as_str().map(|s| avro_name(s)))
                    .collect();
                let mut avro_enum = create_enum_type(&pascal(name), namespace, &symbols);
                merge_description_into_doc(json_object, &mut avro_enum);
                return avro_enum;
            }
        }

        // Derive a base record name once, from title or name
        let title = json_object.get("title").and_then(|t| t.as_str());
        let raw_name = if !name.is_empty() {
            name
        } else if let Some(t) = title {
            t
        } else {
            "" // convert nulls to empty string, avro_name will turn it into "_"
        };
        let record_name = avro_name(raw_name);

        // Arrays
        if is_array_object(json_object) {
            let mut deps = Vec::new();
            let mut array_type = json_type_to_avro_type(
                json_object,
                name,
                &record_name,
                namespace,
                utility_namespace,
                &mut deps,
                json_schema,
                base_uri,
                avro_schema,
                record_stack,
                1,
            );
            if array_type.is_null() {
                array_type = json!({ "type": "null" });
            }

            if let Some(t) = json_object.get("title").and_then(|t| t.as_str()) {
                if let Some(obj) = array_type.as_object_mut() {
                    obj.insert("name".to_string(), json!(avro_name(t)));
                }
            }

            let mut avro_array = create_wrapper_record(
                &(record_name.clone() + "_wrapper"),
                utility_namespace,
                "items",
                &[],
                array_type,
            );
            merge_description_into_doc(json_object, &mut avro_array);
            if avro_array.get("items").is_some() {
                // Move it out
                let mut items_val = avro_array["items"].take();

                // Now no outstanding borrow into avro_array, so safe:
                merge_dependencies_into_parent(&mut deps, &mut items_val, &mut avro_array);

                // Put it back
                avro_array["items"] = items_val;
            }
            return avro_array;
        }

        // Adjust namespace if nested (based on parent, not current)
        let effective_namespace = if let Some(parent) = record_stack.last() {
            crate::converter::utils::compose_namespace(&[namespace, &format!("{}_types", parent)])
        } else {
            namespace.to_string()
        };

        // (IMPORTANT: NO EARLY RETURNS MUST FOLLOW THIS WITHOUT POP)
        record_stack.push(record_name.clone());

        let mut avro_record = create_avro_record(&record_name, &effective_namespace, Vec::new());

        // Collect "required" list from the parent object
        let required_fields: Vec<&str> = json_object
            .get("required")
            .and_then(|r| r.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
            .unwrap_or_default();

        // Handle fields
        if let Some(props) = json_object.get("properties").and_then(|p| p.as_object()) {
            for (field_name, field_schema) in props {
                // Normalize: wrap single object as a one-element array
                let schema_list: Vec<&Value> = if field_schema.is_array() {
                    field_schema.as_array().unwrap().iter().collect()
                } else {
                    vec![field_schema]
                };

                let mut const_val: Option<Value> = None;
                let mut default_val: Option<Value> = None;
                let mut desc_val: Option<String> = None;
                let mut last_avro_type: Option<Value> = None;
                let mut deps = Vec::new();

                for schema_obj in schema_list {
                    if !schema_obj.is_object() {
                        continue;
                    }

                    if let Some(c) = schema_obj.get("const") {
                        const_val = Some(c.clone());
                    }
                    if let Some(d) = schema_obj.get("default") {
                        if !d.is_object() && !d.is_array() {
                            default_val = Some(d.clone());
                        }
                    }
                    if let Some(desc) = schema_obj.get("description").and_then(|d| d.as_str()) {
                        desc_val = Some(desc.to_string());
                    }

                    // Special case $ref
                    let avro_field_type =
                        if let Some(ref_str) = schema_obj.get("$ref").and_then(|r| r.as_str()) {
                            if let Some(def_name) = ref_str.strip_prefix("#/$defs/") {
                                json!(format!("{}.{}", effective_namespace, def_name))
                            } else if let Some(ptr) = ref_str.strip_prefix('#') {
                                if let Some(resolved) = json_schema.pointer(ptr) {
                                    json_schema_object_to_avro_record(
                                        field_name,
                                        resolved,
                                        &effective_namespace,
                                        utility_namespace,
                                        json_schema,
                                        base_uri,
                                        avro_schema,
                                        record_stack,
                                    )
                                } else {
                                    json!("string")
                                }
                            } else {
                                eprintln!("WARN: external $ref not supported: {}", ref_str);
                                json!("string")
                            }
                        } else {
                            json_type_to_avro_type(
                                schema_obj,
                                &record_name,
                                field_name,
                                &effective_namespace,
                                utility_namespace,
                                &mut deps,
                                json_schema,
                                base_uri,
                                avro_schema,
                                record_stack,
                                1,
                            )
                        };

                    last_avro_type = Some(avro_field_type);
                }

                // Pick last type seen (or fallback)
                let mut effective_type = last_avro_type.unwrap_or(json!("string"));

                // Nullable if not required
                if !required_fields.contains(&field_name.as_str()) {
                    match &effective_type {
                        Value::Array(arr) if arr.iter().any(|t| t == "null") => {}
                        _ => {
                            effective_type = json!(["null", effective_type]);
                        }
                    }
                }

                let mut field = json!({
                    "name": field_name,
                    "type": effective_type
                });
                if let Some(c) = const_val {
                    field["const"] = c;
                }
                if let Some(d) = default_val {
                    field["default"] = d;
                }
                if let Some(desc) = desc_val {
                    field["doc"] = Value::String(desc);
                }

                avro_record["fields"].as_array_mut().unwrap().push(field);
                dependencies.extend(deps);
            }
        }

        // Handle extensions: patternProperties & additionalProperties
        let pattern_types = handle_pattern_properties(
            json_object,
            &record_name,
            &effective_namespace,
            utility_namespace,
            base_uri,
            avro_schema,
            record_stack,
            &mut dependencies,
        );
        if !pattern_types.is_empty() {
            avro_record["doc"] =
                Value::String(format!("Pattern properties: {}", pattern_types.len()));
        }

        if let Some(additional) = handle_additional_properties(
            json_object,
            &record_name,
            &effective_namespace,
            utility_namespace,
            base_uri,
            avro_schema,
            record_stack,
            &mut dependencies,
        ) {
            avro_record["doc"] = Value::String(format!(
                "{}; Additional properties allowed",
                avro_record
                    .get("doc")
                    .and_then(|d| d.as_str())
                    .unwrap_or("")
            ));
            avro_record["additionalProperties"] = additional;
        }

        if !dependencies.is_empty() {
            avro_record["dependencies"] =
                Value::Array(dependencies.into_iter().map(Value::String).collect());
        }

        record_stack.pop();

        avro_record
    }

    /// Convert a JSON Schema type into an Avro type.
    pub fn json_type_to_avro_type(
        json_type: &Value,
        record_name: &str,
        field_name: &str,
        namespace: &str,
        utility_namespace: &str,
        dependencies: &mut Vec<String>,
        json_schema: &Value,
        base_uri: &str,
        avro_schema: &mut Vec<Value>,
        record_stack: &mut Vec<String>,
        recursion_depth: usize,
    ) -> Value {
        if recursion_depth >= 40 {
            eprintln!(
                "WARNING: Maximum recursion depth reached for {record_name} at field {field_name}"
            );
            return serde_json::Value::Array(generic_type());
        }

        let local_name = avro_name(if !field_name.is_empty() {
            field_name
        } else {
            record_name
        });
        let avro_type = Value::Null;

        if let Some(obj) = json_type.as_object() {
            let mut json_object_type = obj.get("type").cloned();

            // Handle list-of-types (e.g. "type": ["null","string"])
            if let Some(Value::Array(type_list)) = &json_object_type {
                if type_list.len() == 1 {
                    json_object_type = Some(type_list[0].clone());
                } else if type_list.len() == 2 && type_list.iter().any(|t| t == "null") {
                    let other = type_list.iter().find(|t| *t != "null").unwrap().clone();
                    json_object_type = Some(other);
                } else {
                    let mut one_of = vec![];
                    for t in type_list {
                        if t != "null" {
                            one_of.push(json!({ "type": t }));
                        }
                    }
                    let mut new_obj = obj.clone();
                    new_obj.remove("type");
                    new_obj.insert("oneOf".to_string(), Value::Array(one_of));
                    return json_type_to_avro_type(
                        &Value::Object(new_obj),
                        record_name,
                        field_name,
                        namespace,
                        utility_namespace,
                        dependencies,
                        json_schema,
                        base_uri,
                        avro_schema,
                        record_stack,
                        recursion_depth + 1,
                    );
                }
            }

            // Handle compositions
            if obj.contains_key("allOf") || obj.contains_key("oneOf") || obj.contains_key("anyOf") {
                let merged = merge_json_schemas(&[json_type.clone()], false);
                return json_type_to_avro_type(
                    &merged,
                    record_name,
                    field_name,
                    namespace,
                    utility_namespace,
                    dependencies,
                    json_schema,
                    base_uri,
                    avro_schema,
                    record_stack,
                    recursion_depth + 1,
                );
            }

            // Handle enums
            if let Some(enum_vals) = obj.get("enum").and_then(|v| v.as_array()) {
                let symbols: Vec<String> = enum_vals
                    .iter()
                    .filter_map(|v| v.as_str().map(|s| avro_name(s)))
                    .collect();
                if !symbols.is_empty() {
                    return create_enum_type(
                        &local_name,
                        &format!("{namespace}.{record_name}_types"),
                        &symbols,
                    );
                }
            }

            // Handle arrays
            if json_object_type == Some(Value::String("array".into())) {
                if let Some(items) = obj.get("items") {
                    let mut deps = vec![];
                    let item_type = if items.is_array() {
                        // tuple typing → preserve as-is
                        items.clone()
                    } else {
                        // homogeneous array → recurse
                        json_type_to_avro_type(
                            items,
                            record_name,
                            field_name,
                            namespace,
                            utility_namespace,
                            &mut deps,
                            json_schema,
                            base_uri,
                            avro_schema,
                            record_stack,
                            recursion_depth + 1,
                        )
                    };
                    dependencies.extend(deps);
                    return create_array_type(item_type);
                } else {
                    return create_array_type(serde_json::Value::Array(generic_type()));
                }
            }

            // Handle objects
            if json_object_type == Some(Value::String("object".into())) {
                // Special-case: plain object with only additionalProperties → treat as a map
                if obj.get("properties").is_none() {
                    if let Some(additional) = obj.get("additionalProperties") {
                        if additional.is_boolean() && additional.as_bool().unwrap() {
                            // any-type map
                            return create_map_type(Value::Array(generic_type()), Some(field_name));
                        }
                        if additional.is_object() {
                            let values_type = json_type_to_avro_type(
                                additional,
                                record_name,
                                &(field_name.to_string() + "_values"),
                                namespace,
                                utility_namespace,
                                &mut Vec::new(),
                                json_schema,
                                base_uri,
                                avro_schema,
                                record_stack,
                                recursion_depth + 1,
                            );
                            return create_map_type(values_type, Some(field_name));
                        }
                    }
                }

                // Default: full object with properties, patternProperties, etc.
                return json_schema_object_to_avro_record(
                    &local_name,
                    json_type,
                    namespace,
                    utility_namespace,
                    json_schema,
                    base_uri,
                    avro_schema,
                    record_stack,
                );
            }

            // Handle const → enum
            if let Some(c) = obj.get("const") {
                let values = if c.is_array() {
                    c.as_array().unwrap().clone()
                } else {
                    vec![c.clone()]
                };
                let symbols: Vec<String> = values
                    .iter()
                    .filter_map(|v| v.as_str().map(avro_name))
                    .collect();
                let mut enum_type = create_enum_type(&local_name, namespace, &symbols);
                if let Some(desc) = obj.get("description").and_then(|d| d.as_str()) {
                    enum_type["doc"] = Value::String(desc.to_string());
                }

                return merge_avro_schemas(
                    &[avro_type, enum_type],
                    avro_schema,
                    Some(&local_name),
                    dependencies,
                );
            }

            // Otherwise: primitives
            if let Some(Value::String(t)) = json_object_type {
                let fmt = obj.get("format").and_then(|f| f.as_str());
                let enum_vals = obj.get("enum").and_then(|v| v.as_array());
                let enum_strings = enum_vals.map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect::<Vec<_>>()
                });

                return json_schema_primitive_to_avro_type(
                    &Value::String(t.clone()),
                    fmt,
                    enum_strings
                        .as_ref()
                        .map(|v| {
                            v.iter()
                                .map(|s| Value::String(s.clone()))
                                .collect::<Vec<_>>()
                        })
                        .as_deref(),
                    record_name,
                    field_name,
                    namespace,
                    dependencies,
                );
            }
        }

        // If it wasn't an object, maybe just a primitive string
        if let Some(s) = json_type.as_str() {
            return json_schema_primitive_to_avro_type(
                &Value::String(s.to_string()),
                None,
                None,
                record_name,
                field_name,
                namespace,
                dependencies,
            );
        }

        serde_json::Value::Array(generic_type())
    }
}
pub use innermod::*;
