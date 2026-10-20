use serde_json::Value;

use crate::common::names::avro_name;

/// An object of `entries`, in order.
///
/// Unlike `json!`, which serializes each interpolated value into a copy, this moves
/// the values in.
pub(crate) fn object<const N: usize>(entries: [(&str, Value); N]) -> Value {
    Value::Object(
        entries
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect(),
    )
}

/// Create an Avro record type.
///
/// A record is a structured type with named fields.
pub fn create_avro_record(name: &str, namespace: &str, fields: Vec<Value>) -> Value {
    object([
        ("type", "record".into()),
        ("name", avro_name(name).into()),
        ("namespace", namespace.into()),
        ("fields", Value::Array(fields)),
    ])
}

/// Create a wrapper record around another type.
///
/// Useful when Avro requires a record but the JSON Schema root
/// is a primitive, enum, or array.
pub fn create_wrapper_record(
    wrapper_name: &str,
    wrapper_namespace: &str,
    wrapper_field: &str,
    dependencies: &[String],
    avro_type: Value,
) -> Value {
    let mut record = create_avro_record(
        wrapper_name,
        wrapper_namespace,
        vec![object([
            ("name", wrapper_field.into()),
            ("type", avro_type),
        ])],
    );

    if !dependencies.is_empty() {
        record["dependencies"] = Value::Array(
            dependencies
                .iter()
                .map(|d| Value::String(d.clone()))
                .collect(),
        );
    }

    record
}

/// Create an Avro enum type.
///
/// Symbols are automatically normalized with `avro_name`.
pub fn create_enum_type(name: &str, namespace: &str, symbols: &[String]) -> Value {
    let symbols = symbols
        .iter()
        .map(|s| Value::String(avro_name(s)))
        .collect();
    object([
        ("type", "enum".into()),
        ("name", avro_name(name).into()),
        ("namespace", namespace.into()),
        ("symbols", Value::Array(symbols)),
    ])
}

/// Create an Avro array type.
pub fn create_array_type(items: Value) -> Value {
    object([("type", "array".into()), ("items", items)])
}

/// Create an Avro map type.
pub fn create_map_type(values: Value, name: Option<&str>) -> Value {
    // Construct fields in a fixed order
    if let Some(n) = name {
        object([
            ("name", n.into()),
            ("type", "map".into()),
            ("values", values),
        ])
    } else {
        object([("type", "map".into()), ("values", values)])
    }
}

/// Wrap a type in a union with `null`.
///
/// Avro uses this pattern to make fields nullable.
pub fn nullable(avro_type: Value) -> Value {
    match avro_type {
        Value::Array(arr) => {
            let mut new_arr = vec![Value::String("null".to_string())];
            new_arr.extend(arr);
            Value::Array(new_arr)
        }
        _ => Value::Array(vec![Value::String("null".to_string()), avro_type]),
    }
}
