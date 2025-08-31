use serde_json::Value;

/// Check if the given Avro schema type is empty.
///
/// A type is considered empty if:
/// - It has no entries at all, or
/// - It is a record with no fields, or
/// - It is an enum with no symbols, or
/// - It is an array with no items, or
/// - It is a map with no values.
pub fn is_empty_type(avro_type: &Value) -> bool {
    if avro_type.is_null() {
        return true;
    }
    if avro_type.is_array() {
        return avro_type.as_array().unwrap().iter().all(is_empty_type);
    }
    if avro_type.is_object() {
        let obj = avro_type.as_object().unwrap();
        if !obj.contains_key("type") {
            return true;
        }
        match obj.get("type").and_then(|v| v.as_str()) {
            Some("record") => {
                !obj.contains_key("fields")
                    || obj
                        .get("fields")
                        .and_then(|f| f.as_array())
                        .map_or(true, |f| f.is_empty())
            }
            Some("enum") => {
                !obj.contains_key("symbols")
                    || obj
                        .get("symbols")
                        .and_then(|s| s.as_array())
                        .map_or(true, |s| s.is_empty())
            }
            Some("array") => {
                !obj.contains_key("items") || obj.get("items").map_or(true, |i| is_empty_type(i))
            }
            Some("map") => {
                !obj.contains_key("values") || obj.get("values").map_or(true, |v| is_empty_type(v))
            }
            _ => false,
        }
    } else {
        false
    }
}

/// Check if the given JSON schema type is empty.
///
/// A type is considered empty if:
/// - It has no entries at all, or
/// - It is a list whose elements are all empty types, or
/// - It is an object without a `type` field.
pub fn is_empty_json_type(json_type: &Value) -> bool {
    if json_type.is_null() {
        return true;
    }
    if json_type.is_array() {
        return json_type.as_array().unwrap().iter().all(is_empty_json_type);
    }
    if json_type.is_object() {
        let obj = json_type.as_object().unwrap();
        if obj.is_empty() || !obj.contains_key("type") {
            return true;
        }
    }
    false
}
