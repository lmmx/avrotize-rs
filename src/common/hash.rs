use serde_json::Value;
use std::collections::HashMap;
use xxhash_rust::xxh64::xxh64;

#[derive(Debug, Clone)]
pub struct NodeHash {
    pub hash_value: u64,
    pub count: usize,
}

#[derive(Debug, Clone)]
pub struct NodeHashReference {
    pub hash_value: u64,
    pub count: usize,
    pub value: Value,
    pub path: String,
}

/// Generate a hash from a JSON object (dict, list, or primitive) using xxh64.
pub fn get_tree_hash(json_obj: &Value) -> NodeHash {
    let json_str = if json_obj.is_object() || json_obj.is_array() {
        serde_json::to_string(&json_obj).unwrap()
    } else {
        json_obj.to_string()
    };
    let hash_val = xxh64(json_str.as_bytes(), 0);

    NodeHash {
        hash_value: hash_val,
        count: json_str.len(),
    }
}

/// Build a flat dictionary of hashes for a JSON object.
pub fn build_tree_hash_list(json_obj: &Value, path: &str) -> HashMap<String, NodeHashReference> {
    fn has_nested_structure(obj: &Value) -> bool {
        match obj {
            Value::Object(map) => map.values().any(|v| v.is_object() || v.is_array()),
            Value::Array(arr) => arr.iter().any(|v| v.is_object() || v.is_array()),
            _ => false,
        }
    }

    let mut tree_hash: HashMap<String, NodeHashReference> = HashMap::new();

    match json_obj {
        Value::Object(map) => {
            for (key, value) in map {
                let new_path = if path.is_empty() {
                    format!("$.{}", key)
                } else {
                    format!("{}.{}", path, key)
                };
                if value.is_object() && has_nested_structure(value) {
                    let inner = build_tree_hash_list(value, &new_path);
                    tree_hash.extend(inner);
                    let h = get_tree_hash(value);
                    tree_hash.insert(
                        new_path.clone(),
                        NodeHashReference {
                            hash_value: h.hash_value,
                            count: h.count,
                            value: value.clone(),
                            path: new_path.clone(),
                        },
                    );
                }
            }
        }
        Value::Array(arr) => {
            for (idx, item) in arr.iter().enumerate() {
                let new_path = format!("{}[{}]", path, idx);
                if (item.is_object() || item.is_array()) && has_nested_structure(item) {
                    let inner = build_tree_hash_list(item, &new_path);
                    tree_hash.extend(inner);
                }
            }
        }
        _ => {}
    }

    tree_hash
}

/// Group JSON Path expressions by their hash values.
/// Filters out unique hashes (only returns groups with >1 item).
pub fn group_by_hash(
    tree_hash_list: &HashMap<String, NodeHashReference>,
) -> HashMap<u64, Vec<NodeHashReference>> {
    let mut groups: HashMap<u64, Vec<NodeHashReference>> = HashMap::new();

    for hash_ref in tree_hash_list.values() {
        groups
            .entry(hash_ref.hash_value)
            .or_default()
            .push(hash_ref.clone());
    }

    groups.retain(|_, v| v.len() > 1);
    groups
}
