use serde_json::json;
use serde_json::Value;

/// Construct a generic Avro type union (simple types + arrays + maps).
pub fn generic_type() -> Vec<Value> {
    let simple_type_union: Vec<Value> = vec![
        json!("null"),
        json!("boolean"),
        json!("int"),
        json!("long"),
        json!("float"),
        json!("double"),
        json!("bytes"),
        json!("string"),
    ];

    let mut l2 = simple_type_union.clone();
    l2.extend(vec![
        json!({"type": "array", "items": simple_type_union.clone()}),
        json!({"type": "map", "values": simple_type_union.clone()}),
    ]);

    let mut l1 = simple_type_union.clone();
    l1.extend(vec![
        json!({"type": "array", "items": l2.clone()}),
        json!({"type": "map", "values": l2.clone()}),
    ]);

    l1
}

/// Construct a generic JSON schema type definition.
pub fn generic_type_json() -> Value {
    json!({
        "oneOf": [
            {"type": "boolean"},
            {"type": "integer", "format": "int32"},
            {"type": "integer", "format": "int64"},
            {"type": "number", "format": "float"},
            {"type": "number", "format": "double"},
            {"type": "string", "format": "byte"},
            {"type": "string"},
            {
                "type": "array",
                "items": {
                    "oneOf": [
                        {"type": "boolean"},
                        {"type": "integer", "format": "int32"},
                        {"type": "integer", "format": "int64"},
                        {"type": "number", "format": "float"},
                        {"type": "number", "format": "double"},
                        {"type": "string", "format": "byte"},
                        {"type": "string"}
                    ]
                }
            },
            {
                "type": "object",
                "additionalProperties": {
                    "oneOf": [
                        {"type": "boolean"},
                        {"type": "integer", "format": "int32"},
                        {"type": "integer", "format": "int64"},
                        {"type": "number", "format": "float"},
                        {"type": "number", "format": "double"},
                        {"type": "string", "format": "byte"},
                        {"type": "string"}
                    ]
                }
            }
        ]
    })
}
