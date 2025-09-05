#[cfg_attr(feature = "trace", crustrace::omni)]
mod innermod {
    use serde_json::{json, Value};

    use crate::converter::analysis::is_standalone_avro_type;
    use crate::converter::conversion::json_schema_object_to_avro_record;
    use crate::converter::emptiness::is_empty_type;
    use crate::converter::postprocess::register_type;
    use crate::converter::structs::create_wrapper_record;
    use crate::converter::utils::lift_dependencies_from_type;

    /// Process a schema definition list (e.g. `$defs` or `definitions`).
    pub fn process_definition_list(
        json_schema: &Value,
        namespace: &str,
        utility_namespace: &str,
        base_uri: &str,
        avro_schema: &mut Vec<Value>,
        record_stack: &mut Vec<String>,
        _schema_name: &str,
        json_schema_list: &Value,
    ) {
        if let Some(map) = json_schema_list.as_object() {
            for (sub_schema_name, schema) in map {
                if schema.is_object() {
                    process_definition(
                        json_schema,
                        namespace,
                        utility_namespace,
                        base_uri,
                        avro_schema,
                        record_stack,
                        sub_schema_name,
                        schema,
                        false,
                    );
                }
            }
        }
    }

    /// Process a single schema definition into Avro.
    ///
    /// Returns `(namespace, name)` if a type was registered.
    pub fn process_definition(
        json_schema: &Value,
        namespace: &str,
        utility_namespace: &str,
        base_uri: &str,
        avro_schema: &mut Vec<Value>,
        record_stack: &mut Vec<String>,
        schema_name: &str,
        schema: &Value,
        is_root: bool,
    ) -> Option<(String, String)> {
        if let Some(all_of) = schema.get("allOf").and_then(|a| a.as_array()) {
            // base = schema without "allOf"
            let mut base = schema.clone();
            if let Some(obj) = base.as_object_mut() {
                obj.remove("allOf");
            }

            let mut type_list = vec![base];
            type_list.extend(all_of.iter().cloned());

            // merge_json_schemas already removes conflicts / unions
            let merged = crate::converter::merging::merge_json_schemas(&type_list, false);

            // Now merged has no "allOf" — safe to recurse once
            return process_definition(
                json_schema,
                namespace,
                utility_namespace,
                base_uri,
                avro_schema,
                record_stack,
                schema_name,
                &merged,
                is_root,
            );
        }

        let ty = schema.get("type").and_then(|t| t.as_str());

        let avro_schema_item_list = match ty {
            Some("object") | Some("array") => json_schema_object_to_avro_record(
                schema_name,
                schema,
                namespace,
                utility_namespace,
                json_schema,
                base_uri,
                avro_schema,
                record_stack,
            ),
            Some("string" | "integer" | "number" | "boolean") => {
                let fmt = schema.get("format").and_then(|f| f.as_str());
                let enums = schema.get("enum").and_then(|v| v.as_array());
                crate::converter::types::json_schema_primitive_to_avro_type(
                    &Value::String(ty.unwrap().to_string()),
                    fmt,
                    enums.map(|v| v.as_slice()),
                    schema_name,
                    schema_name,
                    namespace,
                    &mut Vec::new(),
                )
            }
            _ => {
                #[cfg(feature = "trace")]
                tracing::warn!("process_definition: unhandled type {:?}", ty);
                json!("string") // safe fallback
            }
        };

        let mut avro_schema_items = match avro_schema_item_list {
            Value::Array(arr) => arr,
            item if item.is_object() => vec![item],
            _ => {
                return None;
            }
        };

        if is_root && avro_schema_items.len() > 1 {
            // Wrap multiple root-level items
            let wrapper = create_wrapper_record(
                &format!("{schema_name}_wrapper"),
                namespace,
                "root",
                &[],
                Value::Array(avro_schema_items.clone()),
            );
            register_type(avro_schema, wrapper.clone());
            return Some((
                wrapper
                    .get("namespace")
                    .unwrap()
                    .as_str()
                    .unwrap()
                    .to_string(),
                wrapper.get("name").unwrap().as_str().unwrap().to_string(),
            ));
        }

        for mut avro_item in avro_schema_items.drain(..) {
            if let Some(obj) = avro_item.as_object_mut() {
                if !obj.contains_key("name") {
                    obj.insert("name".to_string(), Value::String(schema_name.to_string()));
                }
            }

            let name = avro_item
                .get("name")
                .and_then(|n| n.as_str())
                .unwrap_or(schema_name);
            let ns = avro_item
                .get("namespace")
                .and_then(|n| n.as_str())
                .unwrap_or(namespace);

            if is_standalone_avro_type(&avro_item) && !is_empty_type(&avro_item) {
                register_type(avro_schema, avro_item.clone());
                return Some((ns.to_string(), name.to_string()));
            }

            if is_root {
                let mut deps = Vec::new();
                let mut item_copy = avro_item.clone();
                lift_dependencies_from_type(&mut item_copy, &mut deps);

                let wrapper = create_wrapper_record(schema_name, ns, name, &deps, item_copy);
                register_type(avro_schema, wrapper.clone());
                return Some((
                    wrapper
                        .get("namespace")
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_string(),
                    wrapper.get("name").unwrap().as_str().unwrap().to_string(),
                ));
            }
        }

        None
    }
}
pub use innermod::*;
