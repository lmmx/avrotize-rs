use super::merging::merge_avro_schemas;
use serde_json::Value;

/// Flatten a union type into a simplified list of unique types.
///
/// This will:
/// - Recursively expand nested lists (e.g. unions of unions),
/// - Remove duplicates,
/// - Merge multiple `array` or `map` definitions into one.
pub fn flatten_union(type_list: &[Value], avro_schemas: &[Value]) -> Vec<Value> {
    let mut flat_list: Vec<Value> = Vec::new();

    // Expand nested lists and remove duplicates
    for t in type_list {
        if t.is_array() {
            let inner = flatten_union(t.as_array().unwrap(), avro_schemas);
            for u in inner {
                if !flat_list.contains(&u) {
                    flat_list.push(u);
                }
            }
        } else if !flat_list.contains(t) {
            flat_list.push(t.clone());
        }
    }

    // Consolidate array/map definitions
    let mut array_type: Option<Value> = None;
    let mut map_type: Option<Value> = None;
    let mut flat_list_1: Vec<Value> = Vec::new();

    for t in flat_list {
        if let Some(obj) = t.as_object() {
            if obj.get("type") == Some(&Value::String("array".to_string()))
                && obj.contains_key("items")
            {
                if let Some(existing) = array_type.take() {
                    array_type = Some(merge_avro_schemas(
                        &[existing, t.clone()],
                        avro_schemas,
                        None,
                        &mut Vec::new(),
                    ));
                } else {
                    array_type = Some(t.clone());
                    flat_list_1.push(t.clone());
                }
            } else if obj.get("type") == Some(&Value::String("map".to_string()))
                && obj.contains_key("values")
            {
                if let Some(existing) = map_type.take() {
                    map_type = Some(merge_avro_schemas(
                        &[existing, t.clone()],
                        avro_schemas,
                        None,
                        &mut Vec::new(),
                    ));
                } else {
                    map_type = Some(t.clone());
                    flat_list_1.push(t.clone());
                }
            } else {
                if !flat_list_1.contains(&t) {
                    flat_list_1.push(t.clone());
                }
            }
        } else if !flat_list_1.contains(&t) {
            flat_list_1.push(t.clone());
        }
    }

    flat_list_1
}
