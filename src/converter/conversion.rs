#[cfg_attr(feature = "trace", crustrace::omni)]
mod innermod {
    use crate::common::generic::generic_type;
    use crate::common::names::{avro_name, pascal};
    use crate::converter::analysis::{has_composition_keywords, has_enum_keyword, is_array_object};
    use crate::converter::merging::{merge_avro_schemas, merge_json_schemas};
    use crate::converter::structs::{
        create_array_type, create_avro_record, create_enum_type, create_map_type,
        create_wrapper_record, object,
    };
    use crate::converter::types::json_schema_primitive_to_avro_type;
    use crate::converter::unions::union_members;

    use super::refs::{expand_once, is_definition_ref, recursive_ref};
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
                    // "additionalProperties": true -> a map of any values
                    return Some(object([
                        ("type", "map".into()),
                        ("values", Value::Array(generic_type())),
                    ]));
                }
            } else if additional.is_object() {
                let mut deps = Vec::new();

                // Use parent namespace only: drop the current record from the stack
                let mut parent_stack = record_stack.clone();
                parent_stack.pop();

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
                    &mut parent_stack,
                    1,
                );
                dependencies.extend(deps);
                return Some(object([("type", "map".into()), ("values", avro_type)]));
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
        if json_object.as_object().is_some_and(|obj| obj.is_empty()) {
            return Value::Array(vec![]);
        }
        let mut dependencies: Vec<String> = Vec::new();

        if let Some(ref_str) = json_object.get("$ref").and_then(|r| r.as_str()) {
            if is_definition_ref(ref_str) {
                // Resolved by jsons_to_avro once every definition is registered.
                return Value::String(ref_str.to_string());
            }

            if let Some(ptr) = ref_str.strip_prefix('#') {
                if let Some(resolved) = json_schema.pointer(ptr) {
                    return expand_once(ref_str, || {
                        json_schema_object_to_avro_record(
                            name,
                            resolved,
                            namespace,
                            utility_namespace,
                            json_schema,
                            base_uri,
                            avro_schema,
                            record_stack,
                        )
                    })
                    .unwrap_or_else(|| recursive_ref(ref_str));
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
                    .filter_map(|v| v.as_str().map(avro_name))
                    .collect();
                let mut avro_enum = create_enum_type(&pascal(name), namespace, &symbols);
                merge_description_into_doc(json_object, &mut avro_enum);
                return avro_enum;
            }
        }

        // An object that declares no properties holds any (#12)
        if declares_no_properties(json_object) {
            return create_map_type(Value::Array(generic_type()), None);
        }

        // Derive a base record name once, from title or name
        let title = json_object.get("title").and_then(|t| t.as_str());
        let raw_name = if !name.is_empty() {
            name
        } else {
            title.unwrap_or_default() // convert nulls to empty string, avro_name will turn it into "_"
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
                            if is_definition_ref(ref_str) {
                                // Resolved by jsons_to_avro once every definition is registered.
                                Value::String(ref_str.to_string())
                            } else if let Some(ptr) = ref_str.strip_prefix('#') {
                                if let Some(resolved) = json_schema.pointer(ptr) {
                                    expand_once(ref_str, || {
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
                                    })
                                    .unwrap_or_else(|| recursive_ref(ref_str))
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
                            // Add null to a union rather than nesting the union in one.
                            let mut members = vec!["null".into()];
                            match effective_type {
                                Value::Array(union) => members.extend(union),
                                other => members.push(other),
                            }
                            effective_type = Value::Array(union_members(members));
                        }
                    }
                }

                let mut field = object([
                    ("name", field_name.as_str().into()),
                    ("type", effective_type),
                ]);
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
            let pattern_map = object([
                ("type", "map".into()),
                ("values", Value::Array(union_members(pattern_types))),
                ("name", record_name.as_str().into()),
            ]);
            let fields = avro_record["fields"].as_array_mut().unwrap();
            // The map is named after the record, unless a property already took that name.
            let taken = |n: &str| fields.iter().any(|f| f["name"] == n);
            let field_name = std::iter::once(record_name.clone())
                .chain((1..).map(|i| format!("{record_name}_{i}")))
                .find(|n| !taken(n))
                .unwrap();
            fields.push(object([("name", field_name.into()), ("type", pattern_map)]));
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
            let has_fields = avro_record["fields"]
                .as_array()
                .map(|a| !a.is_empty())
                .unwrap_or(false);

            if !has_fields {
                // Case: only `additionalProperties` → return the map itself.
                record_stack.pop();
                return additional;
            }

            // Case: properties + additionalProperties → keep record, annotate.
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

    /// Whether `json_object` is an object schema with none of `properties`,
    /// `patternProperties` and `additionalProperties`: it allows any properties, so a
    /// record (which would have no fields) would drop them all. `minProperties` and
    /// `maxProperties` don't change that, and Avro can't express them.
    fn declares_no_properties(json_object: &Value) -> bool {
        json_object.get("type").and_then(Value::as_str) == Some("object")
            && ["properties", "patternProperties", "additionalProperties"]
                .iter()
                .all(|k| json_object.get(*k).is_none())
    }

    /// Keywords of a schema with `oneOf` or `anyOf` that its branches don't take on:
    /// the branches themselves, annotations, and values (an enum of a type list is
    /// converted as the list's types).
    const NOT_FOR_BRANCHES: [&str; 11] = [
        "oneOf",
        "anyOf",
        "description",
        "title",
        "$comment",
        "examples",
        "default",
        "$id",
        "$schema",
        "enum",
        "const",
    ];

    /// What each branch of a `oneOf` or `anyOf` in `obj` takes on from `obj` (its type,
    /// properties, required, ...), as JSON Schema applies both; `None` if nothing.
    fn composition_base(obj: &serde_json::Map<String, Value>) -> Option<Value> {
        let base: serde_json::Map<String, Value> = obj
            .iter()
            .filter(|(k, _)| !NOT_FOR_BRANCHES.contains(&k.as_str()))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        (!base.is_empty()).then_some(Value::Object(base))
    }

    /// Whether `base` can be merged into the branch `sub`: not into a reference, and
    /// not into a branch of another type, which stays as it is (as in Python avrotize).
    fn merges_into(base: &Value, sub: &Value) -> bool {
        sub.get("$ref").is_none()
            && match (base.get("type"), sub.get("type")) {
                (Some(t1), Some(t2)) => t1 == t2,
                _ => true,
            }
    }

    /// The schema an `allOf` describes: the schema without its `allOf`, merged with
    /// each member, a member's local `$ref` merged with what it points at.
    fn merge_all_of(json_type: &Value, json_schema: &Value) -> Value {
        let mut base = json_type.clone();
        let members = base
            .as_object_mut()
            .and_then(|obj| obj.remove("allOf"))
            .and_then(|all_of| match all_of {
                Value::Array(members) => Some(members),
                _ => None,
            })
            .unwrap_or_default();
        let mut type_list = vec![base];
        for mut member in members {
            // A bound, as a reference may lead back to itself.
            for _ in 0..32 {
                let Some(resolved) = member
                    .get("$ref")
                    .and_then(Value::as_str)
                    .and_then(|r| r.strip_prefix('#'))
                    .and_then(|pointer| json_schema.pointer(pointer))
                else {
                    break;
                };
                let resolved = resolved.clone();
                member.as_object_mut().unwrap().remove("$ref");
                member = merge_json_schemas(&[member, resolved], false);
            }
            type_list.push(member);
        }
        merge_json_schemas(&type_list, false)
    }

    /// The Avro type of a non-string `const` value, mapped as its JSON type would be.
    fn const_type(value: &Value) -> Option<&'static str> {
        match value {
            Value::Null => Some("null"),
            Value::Bool(_) => Some("boolean"),
            Value::Number(n) if n.is_i64() || n.is_u64() => {
                let fits_int = n.as_i64().is_some_and(|i| i32::try_from(i).is_ok());
                Some(if fits_int { "int" } else { "long" })
            }
            Value::Number(_) => Some("float"),
            _ => None,
        }
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
            // A reference on its own (in items, a union branch, additionalProperties ...)
            if let Some(ref_str) = obj.get("$ref").and_then(|r| r.as_str()) {
                if !obj.contains_key("type") && !has_composition_keywords(json_type) {
                    if is_definition_ref(ref_str) {
                        return Value::String(ref_str.to_string());
                    }
                    if let Some(resolved) = ref_str
                        .strip_prefix('#')
                        .and_then(|p| json_schema.pointer(p))
                    {
                        return expand_once(ref_str, || {
                            json_type_to_avro_type(
                                resolved,
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
                            )
                        })
                        .unwrap_or_else(|| recursive_ref(ref_str));
                    }
                    eprintln!("WARN: external $ref not supported: {}", ref_str);
                    return json!("string");
                }
            }

            let mut json_object_type = obj.get("type").cloned();

            // Handle list-of-types (e.g. "type": ["null","string"])
            if let Some(Value::Array(type_list)) = &json_object_type {
                let others: Vec<Value> =
                    type_list.iter().filter(|t| *t != "null").cloned().collect();
                if type_list.len() == 1 {
                    json_object_type = Some(type_list[0].clone());
                } else if others.len() < type_list.len() && !others.is_empty() {
                    // The list admits null: convert the other types, then add null to them.
                    let mut without_null = obj.clone();
                    let rest = match <[Value; 1]>::try_from(others) {
                        Ok([single]) => single,
                        Err(others) => Value::Array(others),
                    };
                    without_null.insert("type".to_string(), rest);
                    let inner = json_type_to_avro_type(
                        &Value::Object(without_null),
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
                    let mut members = vec![Value::from("null")];
                    match inner {
                        Value::Array(union) => members.extend(union),
                        other => members.push(other),
                    }
                    return Value::Array(union_members(members));
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
            if let Some(subs) = obj
                .get("oneOf")
                .and_then(|v| v.as_array())
                .or_else(|| obj.get("anyOf").and_then(|v| v.as_array()))
            {
                let base = composition_base(obj);
                let intersect = obj.contains_key("oneOf");
                let mut union_types = Vec::new();
                for sub in subs {
                    let merged = base
                        .as_ref()
                        .filter(|base| merges_into(base, sub))
                        .map(|base| merge_json_schemas(&[base.clone(), sub.clone()], intersect));
                    let avro_ty = json_type_to_avro_type(
                        merged.as_ref().unwrap_or(sub),
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
                    match avro_ty {
                        Value::Array(mut arr) => union_types.append(&mut arr),
                        other => union_types.push(other),
                    }
                }
                return Value::Array(union_members(union_types));
            }

            if obj.contains_key("allOf") {
                let merged = merge_all_of(json_type, json_schema);
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
                    .filter_map(|v| v.as_str().map(avro_name))
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
                // An object that declares no properties holds any (#12)
                if declares_no_properties(json_type) {
                    return create_map_type(Value::Array(generic_type()), Some(field_name));
                }

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
                if symbols.is_empty() {
                    // A number, boolean or null constant can't be an enum symbol: give
                    // the field its value's type (the field keeps the `const` itself).
                    // An object or array constant has no exact Avro type; the generic
                    // type can hold it.
                    if let [scalar] = values.as_slice() {
                        if let Some(t) = const_type(scalar) {
                            return Value::String(t.to_string());
                        }
                    }
                    return Value::Array(generic_type());
                }
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

/// Reference helpers, outside `innermod`: the `trace` feature instruments every function
/// there, which needs `Debug` arguments, and `expand_once` takes a closure.
mod refs {
    use crate::common::generic::generic_type;
    use serde_json::Value;

    /// Whether `ref_str` points at an entry of `$defs`, e.g. `#/$defs/address`.
    ///
    /// Conversion leaves such a reference as it is; `jsons_to_avro` replaces it with
    /// the definition's registered name, or with its type for a definition that is not
    /// a named type.
    pub(crate) fn is_definition_ref(ref_str: &str) -> bool {
        ref_str
            .strip_prefix("#/$defs/")
            .is_some_and(|name| !name.is_empty() && !name.contains('/'))
    }

    /// Whether `ref_str` points at an entry of `definitions` or `$defs`.
    pub(crate) fn is_any_definition_ref(ref_str: &str) -> bool {
        is_definition_ref(ref_str)
            || ref_str
                .strip_prefix("#/definitions/")
                .is_some_and(|name| !name.is_empty() && !name.contains('/'))
    }

    thread_local! {
        /// The pointer references being expanded inline on this thread, outermost first.
        static EXPANDING: std::cell::RefCell<Vec<String>> =
            const { std::cell::RefCell::new(Vec::new()) };
    }

    /// Run `expand` with `ref_str` marked as being expanded, or return `None` if it
    /// already is: expanding it again would recurse without end.
    pub(crate) fn expand_once<T>(ref_str: &str, expand: impl FnOnce() -> T) -> Option<T> {
        struct Done;
        impl Drop for Done {
            fn drop(&mut self) {
                EXPANDING.with(|e| e.borrow_mut().pop());
            }
        }
        if EXPANDING.with(|e| e.borrow().iter().any(|r| r == ref_str)) {
            return None;
        }
        EXPANDING.with(|e| e.borrow_mut().push(ref_str.to_string()));
        let _done = Done;
        Some(expand())
    }

    /// The type for a reference met again while it is being expanded: a reference to
    /// the definition, which jsons_to_avro resolves to its registered name, or the
    /// generic type when the reference points elsewhere.
    pub(crate) fn recursive_ref(ref_str: &str) -> Value {
        if is_any_definition_ref(ref_str) {
            Value::String(ref_str.to_string())
        } else {
            Value::Array(generic_type())
        }
    }
}

pub(crate) use refs::{expand_once, is_any_definition_ref};
