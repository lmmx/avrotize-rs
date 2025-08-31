use serde_json::Value;

/// Return a readable type name for a field’s Avro type.
///
/// If the type is a string → return directly.
/// If it’s a union (list) → join names with `,`.
/// If it’s a dict → return the dict’s `"type"` value.
/// Otherwise returns `"union"`.
pub fn get_field_type_name(field: &Value) -> String {
    match field.get("type") {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(arr)) => {
            let names: Vec<String> = arr
                .iter()
                .map(|t| {
                    if let Some(s) = t.as_str() {
                        s.to_string()
                    } else if let Some(obj) = t.as_object() {
                        get_field_type_name(&Value::Object(obj.clone()))
                    } else {
                        "union".to_string()
                    }
                })
                .collect();
            names.join(", ")
        }
        Some(Value::Object(map)) => map
            .get("type")
            .and_then(|t| t.as_str())
            .unwrap_or("union")
            .to_string(),
        _ => "union".to_string(),
    }
}

/// Check if a JSON object has composition keywords: allOf, oneOf, anyOf.
pub fn has_composition_keywords(json_object: &Value) -> bool {
    json_object.is_object()
        && (json_object.get("allOf").is_some()
            || json_object.get("oneOf").is_some()
            || json_object.get("anyOf").is_some())
}

/// Check if a JSON object is an enum.
pub fn has_enum_keyword(json_object: &Value) -> bool {
    json_object.is_object() && json_object.get("enum").is_some()
}

/// Check if a JSON object represents an array.
pub fn is_array_object(json_object: &Value) -> bool {
    json_object
        .get("type")
        .and_then(|t| t.as_str())
        .map(|t| t == "array")
        .unwrap_or(false)
}

/// Check if an Avro type is standalone (record, enum, fixed).
pub fn is_standalone_avro_type(avro_type: &Value) -> bool {
    avro_type
        .get("type")
        .and_then(|t| t.as_str())
        .map(|t| t == "record" || t == "enum" || t == "fixed")
        .unwrap_or(false)
}

/// Check if an Avro type is complex (record, enum, fixed, array, map).
pub fn is_avro_complex_type(avro_type: &Value) -> bool {
    avro_type
        .get("type")
        .and_then(|t| t.as_str())
        .map(|t| matches!(t, "record" | "enum" | "fixed" | "array" | "map"))
        .unwrap_or(false)
}