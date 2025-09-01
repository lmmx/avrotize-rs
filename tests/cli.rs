#![cfg(feature = "cli")]
use assert_cmd::Command;
use insta::assert_json_snapshot;
use rstest::rstest;
use std::fs;
use tempfile::tempdir;

fn run_fixture(schema_path: &str, stem: &str) {
    let dir = tempdir().unwrap();
    let input_path = dir.path().join(format!("{stem}.json"));
    let output_path = dir.path().join(format!("{stem}.avsc"));

    // Load schema and copy into tmpdir
    let schema = fs::read_to_string(schema_path).unwrap();
    fs::write(&input_path, schema).unwrap();

    // Run CLI
    Command::cargo_bin("jsonschema2avro")
        .unwrap()
        .arg(input_path.to_str().unwrap())
        .arg(output_path.to_str().unwrap())
        .assert()
        .success();

    // Read output Avro schema
    let output = fs::read_to_string(&output_path).unwrap();
    let json: serde_json::Value = serde_json::from_str(&output).unwrap();

    // Compare with snapshot
    assert_json_snapshot!(stem, json);
}

#[rstest]
#[case("basic_string_schema")]
#[case("basic_string_schema_with_title")]
#[case("nested_object_and_array")]
#[case("object_with_boolean_and_number")]
#[case("enum_string_property")]
#[case("array_of_objects")]
#[case("object_with_optional")]
fn cli_fixtures(#[case] stem: &str) {
    let schema_path = format!("tests/fixtures/jsonschema/{stem}.json");
    run_fixture(&schema_path, stem);
}
// #[case("object_with_defs")]
