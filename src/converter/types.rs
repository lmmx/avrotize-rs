use crate::common::generic_type;
use serde_json::{json, Value};

/// Ensure the given type has a `"type"` field if required.
///
/// If `type` is already a string, list, or contains `"type"`, it is returned unchanged.
/// Otherwise, it is given a generic Avro type placeholder.
pub fn ensure_type(value: &Value) -> Value {
    if value.is_string() || value.is_array() {
        return value.clone();
    }
    if let Some(obj) = value.as_object() {
        if obj.contains_key("type") {
            return value.clone();
        }
    }

    let mut ensured = value.clone();
    if let Some(obj) = ensured.as_object_mut() {
        obj.insert("type".to_string(), json!(generic_type()));
    }
    ensured
}

/// Convert a JSON Schema primitive into an Avro primitive.
///
/// Handles:
/// - `"string"`, `"integer"`, `"number"`, `"boolean"`
/// - JSON Schema `format` annotations (`date-time`, `time`, `duration`, `uuid`)
/// - Enum → Avro enum
pub fn json_schema_primitive_to_avro_type(
    json_primitive: &Value,
    format: Option<&str>,
    enum_values: Option<&[Value]>,
    record_name: &str,
    field_name: &str,
    namespace: &str,
    dependencies: &mut Vec<String>,
) -> Value {
    if json_primitive.is_array() {
        // Union type
        let mut union = Vec::new();
        for item in json_primitive.as_array().unwrap() {
            let enum2 = item.get("enum").and_then(|v| v.as_array());
            let format2 = item.get("format").and_then(|v| v.as_str());
            let subtype = json_schema_primitive_to_avro_type(
                item,
                format2,
                enum2.map(|arr| arr.as_slice()),
                record_name,
                field_name,
                namespace,
                dependencies,
            );
            union.push(subtype);
        }
        return Value::Array(union);
    }

    let primitive_str = json_primitive.as_str().unwrap_or("");

    let mut avro_type = match primitive_str {
        "string" => Value::String("string".to_string()),
        "integer" => {
            if format == Some("int64") {
                Value::String("long".to_string())
            } else {
                Value::String("int".to_string())
            }
        }
        "number" => Value::String("float".to_string()),
        "boolean" => Value::String("boolean".to_string()),
        other => {
            if !other.is_empty() {
                dependencies.push(other.to_string());
            }
            Value::String(other.to_string())
        }
    };

    if let Some(fmt) = format {
        match fmt {
            "date-time" | "date" => {
                avro_type = json!({"type": "int", "logicalType": "date"});
            }
            "time" => {
                avro_type = json!({"type": "int", "logicalType": "time-millis"});
            }
            "duration" => {
                avro_type = json!({"type": "fixed", "size": 12, "logicalType": "duration"});
            }
            "uuid" => {
                avro_type = json!({"type": "string", "logicalType": "uuid"});
            }
            _ => {}
        }
    }

    // Enum values override primitive if present
    if let Some(enum_vals) = enum_values {
        let symbols: Vec<String> = enum_vals
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect();
        if !symbols.is_empty() {
            avro_type = json!({
                "type": "enum",
                "name": format!("{}_{}", record_name, field_name),
                "namespace": namespace,
                "symbols": symbols
            });
        }
    }

    avro_type
}
