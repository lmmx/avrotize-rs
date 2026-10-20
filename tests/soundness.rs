//! Soundness regressions: the converter must write well-formed Avro, and a JSON value
//! valid under the JSON Schema must fit the Avro schema without loss.
mod common;

use avrotize::converter::jsons_to_avro;
use common::{assert_well_formed, fits, named_types};
use serde_json::{json, Value};

fn convert(schema: &Value) -> Value {
    let avro = jsons_to_avro(schema, "ns", "", "", false);
    assert_well_formed(&avro);
    avro
}

/// Assert that `datum` fits `avro`, naming the schema in the failure.
fn assert_fits(avro: &Value, datum: Value) {
    let names = named_types(avro);
    assert!(fits(avro, &datum, &names), "{datum} does not fit {avro:#}");
}

#[test]
fn every_fixture_is_well_formed() {
    for entry in std::fs::read_dir("tests/fixtures/jsonschema").unwrap() {
        let path = entry.unwrap().path();
        let stem = path.file_stem().unwrap().to_str().unwrap().to_string();
        if stem == "recursive_ref" {
            continue; // fetches its $ref over the network
        }
        let schema: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let avro = jsons_to_avro(&schema, &stem, "", "", false);
        let checked = std::panic::catch_unwind(|| assert_well_formed(&avro));
        assert!(checked.is_ok(), "{stem} is not well formed:\n{avro:#}");
    }
}

#[test]
fn formats_without_a_logical_type_keep_the_primitive_type() {
    // Each of these was an empty union, `[]`, which can hold nothing.
    let schema = json!({"type": "object", "required": ["e", "u", "h", "big", "small", "x"],
        "properties": {
            "e": {"type": "string", "format": "email"},
            "u": {"type": "string", "format": "uri"},
            "h": {"type": "string", "format": "hostname"},
            "big": {"type": "integer", "format": "int64"},
            "small": {"type": "integer", "format": "int32"},
            "x": {"type": "number", "format": "double"},
            "opt": {"type": "string", "format": "email"}}});
    let avro = convert(&schema);
    let types: Vec<&Value> = avro["fields"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| &f["type"])
        .collect();
    assert_eq!(
        types,
        [
            &json!("string"),
            &json!("string"),
            &json!("string"),
            &json!("long"),
            &json!("int"),
            &json!("float"),
            &json!(["null", "string"])
        ]
    );
    assert_fits(
        &avro,
        json!({"e": "a@b.c", "u": "http://x", "h": "example.com", "big": 1_099_511_627_776_i64,
            "small": 3, "x": 0.5, "opt": null}),
    );
}

#[test]
fn formats_with_a_logical_type_are_unchanged() {
    let schema = json!({"type": "object", "required": ["d", "t", "id"], "properties": {
        "d": {"type": "string", "format": "date"},
        "t": {"type": "string", "format": "time"},
        "id": {"type": "string", "format": "uuid"}}});
    let avro = convert(&schema);
    assert_eq!(
        avro["fields"],
        json!([
            {"name": "d", "type": {"type": "int", "logicalType": "date"}},
            {"name": "t", "type": {"type": "int", "logicalType": "time-millis"}},
            {"name": "id", "type": {"type": "string", "logicalType": "uuid"}}])
    );
}

#[test]
fn a_formatted_branch_of_a_union_is_kept() {
    // The int64 branch used to convert to `[]`, and splicing it into the union dropped it.
    let schema = json!({"type": "object", "required": ["v"], "properties": {"v": {"oneOf": [
        {"type": "integer", "format": "int64"}, {"type": "boolean"}]}}});
    let avro = convert(&schema);
    assert_eq!(avro["fields"][0]["type"], json!(["long", "boolean"]));
    assert_fits(&avro, json!({"v": 1_099_511_627_776_i64}));
}

#[test]
fn an_optional_union_gains_null_instead_of_nesting() {
    // Was ["null", ["string", "int"]]: a union directly inside a union.
    let schema = json!({"type": "object", "properties": {"o": {"oneOf": [
        {"type": "string"}, {"type": "integer"}]}}});
    let avro = convert(&schema);
    assert_eq!(avro["fields"][0]["type"], json!(["null", "string", "int"]));
    assert_fits(&avro, json!({"o": null}));
    assert_fits(&avro, json!({"o": 3}));
}

#[test]
fn a_repeated_union_member_is_dropped() {
    // Was ["null", "boolean", "null"].
    let schema = json!({"type": "object", "required": ["n"], "properties": {"n": {"oneOf": [
        {"type": "null"}, {"type": "boolean"}, {"type": "null"}]}}});
    let avro = convert(&schema);
    assert_eq!(avro["fields"][0]["type"], json!(["null", "boolean"]));
}

#[test]
fn arrays_in_a_union_merge_into_one_array_of_their_items() {
    // Was two array members, which a union may not hold; Python avrotize keeps only the
    // first, dropping the integer items.
    let schema = json!({"type": "object", "required": ["two"], "properties": {"two": {"anyOf": [
        {"type": "array", "items": {"type": "string"}},
        {"type": "array", "items": {"type": "integer"}}]}}});
    let avro = convert(&schema);
    assert_eq!(
        avro["fields"][0]["type"],
        json!([{"type": "array", "items": ["string", "int"]}])
    );
    assert_fits(&avro, json!({"two": ["a", 1]}));
}

#[test]
fn a_non_string_const_takes_its_value_type() {
    // Each was an enum with no symbols, which is invalid and can hold nothing.
    let schema = json!({"type": "object", "required": ["i", "big", "f", "b", "n", "s"],
        "properties": {
            "i": {"const": 7},
            "big": {"const": 1_099_511_627_776_i64},
            "f": {"const": 0.5},
            "b": {"const": true},
            "n": {"const": null},
            "s": {"const": "x"}}});
    let avro = convert(&schema);
    let fields = avro["fields"].as_array().unwrap();
    let types: Vec<&Value> = fields.iter().map(|f| &f["type"]).collect();
    assert_eq!(
        types[..5],
        [
            &json!("int"),
            &json!("long"),
            &json!("float"),
            &json!("boolean"),
            &json!("null")
        ]
    );
    // A string constant is still a one-symbol enum, and every field keeps its const.
    assert_eq!(types[5]["type"], "enum");
    assert_eq!(fields[0]["const"], 7);
    assert_fits(
        &avro,
        json!({"i": 7, "big": 1_099_511_627_776_i64, "f": 0.5, "b": true, "n": null, "s": "x"}),
    );

    // An object or array constant has no exact Avro type: the generic type holds it.
    let schema = json!({"type": "object", "required": ["o", "a"], "properties": {
        "o": {"const": {"k": "v"}}, "a": {"const": [1, 2]}}});
    let avro = convert(&schema);
    assert_fits(&avro, json!({"o": {"k": "v"}, "a": [1, 2]}));
}
