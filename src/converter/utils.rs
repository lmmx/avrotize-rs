use serde_json::Value;

use std::path::Path;
use url::Url;

use crate::common::names::avro_namespace;

/// Compose a namespace string from multiple parts.
///
/// Empty parts are skipped. Each part is normalized with `avro_namespace`.
pub fn compose_namespace(parts: &[&str]) -> String {
    parts
        .iter()
        .filter(|p| !p.is_empty())
        .map(|p| avro_namespace(p))
        .collect::<Vec<_>>()
        .join(".")
}

/// Get the fully qualified Avro type name: `namespace.name`.
pub fn get_qualified_name(avro_type: &Value) -> String {
    let namespace = avro_type
        .get("namespace")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let name = avro_type.get("name").and_then(|v| v.as_str()).unwrap_or("");
    compose_namespace(&[namespace, name])
}

/// Merge a description from JSON Schema into an Avro type’s `doc` field.
pub fn merge_description_into_doc(source_json: &Value, target_avro: &mut Value) {
    if let Some(desc) = source_json.get("description").and_then(|d| d.as_str()) {
        if let Some(obj) = target_avro.as_object_mut() {
            let new_doc = if let Some(existing) = obj.get("doc").and_then(|d| d.as_str()) {
                format!("{}, {}", existing, desc)
            } else {
                desc.to_string()
            };
            obj.insert("doc".to_string(), Value::String(new_doc));
        }
    }
}

/// Merge child dependencies into a parent Avro type.
///
/// Ensures all dependencies are listed on the parent.
pub fn merge_dependencies_into_parent(
    dependencies: &mut Vec<String>,
    child_type: &mut Value,
    parent_type: &mut Value,
) {
    lift_dependencies_from_type(child_type, dependencies);
    if !dependencies.is_empty() {
        if let Some(obj) = parent_type.as_object_mut() {
            if let Some(existing) = obj.get_mut("dependencies") {
                if let Some(arr) = existing.as_array_mut() {
                    for dep in dependencies.drain(..) {
                        if !arr.iter().any(|v| v.as_str() == Some(&dep)) {
                            arr.push(Value::String(dep));
                        }
                    }
                }
            } else {
                obj.insert(
                    "dependencies".to_string(),
                    Value::Array(dependencies.drain(..).map(Value::String).collect()),
                );
            }
        }
    }
}

/// Lift dependencies from a type into a caller-owned vector.
///
/// Removes the `dependencies` key from the child type if present.
pub fn lift_dependencies_from_type(avro_type: &mut Value, dependencies: &mut Vec<String>) {
    if let Some(obj) = avro_type.as_object_mut() {
        if let Some(deps) = obj.remove("dependencies") {
            if let Some(arr) = deps.as_array() {
                for dep in arr {
                    if let Some(s) = dep.as_str() {
                        dependencies.push(s.to_string());
                    }
                }
            }
        }
    }
}

/// Convert a JSON Schema `$id` URI into an Avro namespace.
/// Mirrors the Python implementation.
pub fn id_to_avro_namespace(id: &str) -> String {
    if let Ok(parsed_url) = Url::parse(id) {
        // Path → strip extension, replace `-` with `_`, split, reverse
        let path_no_ext = {
            let path = parsed_url.path().trim_matches('/');
            // Take only the part before the first dot
            let before_dot = path.split('.').next().unwrap_or("");
            before_dot.replace('-', "_")
        };
        let path_segments: Vec<&str> = path_no_ext.split('/').filter(|s| !s.is_empty()).collect();
        let reversed_path_segments: Vec<&str> = path_segments.into_iter().rev().collect();
        let namespace_suffix = compose_namespace(&reversed_path_segments);

        // Host → reversed segments
        let namespace_prefix = parsed_url
            .host_str()
            .map(|h| {
                let parts: Vec<&str> = h.split('.').rev().collect();
                compose_namespace(&parts)
            })
            .unwrap_or_default();

        // Combine prefix + suffix
        if namespace_prefix.is_empty() {
            namespace_suffix
        } else if namespace_suffix.is_empty() {
            namespace_prefix
        } else {
            compose_namespace(&[&namespace_prefix, &namespace_suffix])
        }
    } else {
        "".to_string() // let caller decide fallback (e.g. filename stem)
    }
}
