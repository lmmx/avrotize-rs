use serde_json::Value;

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
