use serde_json::Value;
use std::collections::HashSet;

/// Recursively adjust resolved dependencies so records are defined before use.
///
/// The first definition of each record, enum or fixed full name stays where it is,
/// and every later one is replaced by a reference to that name.
pub fn adjust_resolved_dependencies(avro_schema: &mut Value) {
    struct TreeWalker {
        found_something: bool,
    }

    impl TreeWalker {
        fn walk_schema(
            &mut self,
            current_node: &mut Value,
            record_list: &mut HashSet<String>,
        ) -> Option<String> {
            if let Some(obj) = current_node.as_object() {
                if let Some(t) = obj.get("type").and_then(|v| v.as_str()) {
                    if t == "record" || t == "enum" || t == "fixed" {
                        let qname = format!(
                            "{}.{}",
                            obj.get("namespace").and_then(|n| n.as_str()).unwrap_or(""),
                            obj.get("name").and_then(|n| n.as_str()).unwrap_or("")
                        );
                        if record_list.contains(&qname) {
                            self.found_something = true;
                            return Some(qname);
                        }
                        record_list.insert(qname);
                    }
                }
            }

            if let Some(obj) = current_node.as_object_mut() {
                // const and default values are data, not types to resolve or dedup.
                for (_, v) in obj
                    .iter_mut()
                    .filter(|(k, _)| *k != "const" && *k != "default")
                {
                    if v.is_object() || v.is_array() {
                        if let Some(qname) = self.walk_schema(v, record_list) {
                            self.found_something = true;
                            *v = Value::String(qname);
                        }
                    }
                }
            } else if let Some(arr) = current_node.as_array_mut() {
                for item in arr.iter_mut() {
                    if item.is_object() || item.is_array() {
                        if let Some(qname) = self.walk_schema(item, record_list) {
                            self.found_something = true;
                            *item = Value::String(qname);
                        }
                    }
                }
                arr.dedup();
            }

            None
        }
    }

    let mut walker = TreeWalker {
        found_something: true,
    };
    loop {
        walker.found_something = false;
        walker.walk_schema(avro_schema, &mut HashSet::new());
        if !walker.found_something {
            break;
        }
    }
}

/// Inline all dependent records to break circular dependencies.
pub fn inline_dependencies_of(avro_schema: &mut Vec<Value>, record: &mut Value) {
    if let Some(deps) = record.get("dependencies").and_then(|d| d.as_array()) {
        let deps_copy: Vec<String> = deps
            .iter()
            .filter_map(|d| d.as_str().map(|s| s.to_string()))
            .collect();

        for dependency in deps_copy {
            if let Some(dep_type) = avro_schema.iter().find(|x| {
                x.get("name").and_then(|n| n.as_str()) == Some(dependency.as_str())
                    || x.get("namespace")
                        .and_then(|n| n.as_str())
                        .map(|ns| format!("{}.{}", ns, x.get("name").unwrap().as_str().unwrap()))
                        == Some(dependency.clone())
            }) {
                let dep_clone = dep_type.clone();
                if let Some(fields) = record.get_mut("fields").and_then(|f| f.as_array_mut()) {
                    for field in fields.iter_mut() {
                        swap_dependency_type(avro_schema, field, &dependency, &dep_clone);
                    }
                }
            }
        }
    }
    if record.get("dependencies").is_some() {
        record.as_object_mut().unwrap().remove("dependencies");
    }
    adjust_resolved_dependencies(record);
}

/// Sort messages by dependencies, inlining when needed.
pub fn sort_messages_by_dependencies(avro_schema: &mut Vec<Value>) -> Vec<Value> {
    if avro_schema.iter().all(|r| r.is_string()) {
        return avro_schema.clone();
    }

    let mut sorted_messages: Vec<Value> = Vec::new();

    while !avro_schema.is_empty() {
        let mut found = false;
        let mut i = 0;
        while i < avro_schema.len() {
            let record = &avro_schema[i];
            if !record.is_object() {
                sorted_messages.push(avro_schema.remove(i));
                found = true;
                continue;
            }
            let deps: Vec<String> = record
                .get("dependencies")
                .and_then(|d| d.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|x| x.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default();
            let remaining_deps: Vec<String> = deps
                .into_iter()
                .filter(|d| {
                    !sorted_messages.iter().any(|s| {
                        s.get("name").map(|n| n.as_str().unwrap()) == Some(d.as_str())
                            || s.get("namespace").and_then(|n| n.as_str()).map(|ns| {
                                format!("{}.{}", ns, s.get("name").unwrap().as_str().unwrap())
                            }) == Some(d.clone())
                    })
                })
                .collect();

            if remaining_deps.is_empty() {
                let mut record_mut = avro_schema.remove(i);
                record_mut.as_object_mut().unwrap().remove("dependencies");
                sorted_messages.push(record_mut);
                found = true;
                continue;
            }
            i += 1;
        }

        if !found {
            // Fallback: break circular dependencies by inlining
            if let Some(idx) = avro_schema
                .iter()
                .position(|r| r.get("dependencies").is_some())
            {
                let mut record = avro_schema.remove(idx);
                inline_dependencies_of(&mut sorted_messages, &mut record);
                sorted_messages.push(record);
            } else {
                eprintln!("WARNING: Circular dependencies remain unresolved.");
                break;
            }
        }
    }

    sorted_messages
}

/// Helper: swap dependency type inside a field.
fn swap_dependency_type(
    _avro_schema: &mut Vec<Value>,
    field: &mut Value,
    dependency: &str,
    dependency_type: &Value,
) {
    if let Some(ftype) = field.get_mut("type") {
        if ftype.is_string() && ftype.as_str() == Some(dependency) {
            *ftype = dependency_type.clone();
        } else if ftype.is_array() {
            if let Some(arr) = ftype.as_array_mut() {
                for item in arr.iter_mut() {
                    if item.as_str() == Some(dependency) {
                        *item = dependency_type.clone();
                    } else if item.is_object() {
                        swap_dependency_type(_avro_schema, item, dependency, dependency_type);
                    }
                }
            }
        } else if ftype.is_object() {
            swap_dependency_type(_avro_schema, ftype, dependency, dependency_type);
        }
    }
}
