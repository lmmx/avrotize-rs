use assert_cmd::Command;
use tempfile::tempdir;
use std::fs;
use insta::assert_json_snapshot;

#[test]
fn cli_basic_string_schema() {
    let dir = tempdir().unwrap();
    let input_path = dir.path().join("schema.json");
    let output_path = dir.path().join("schema.avsc");

    // Load JSON Schema from fixture
    let schema = include_str!("fixtures/jsonschema/basic_string_schema.json");
    fs::write(&input_path, schema).unwrap();

    // Run the CLI
    Command::cargo_bin("jsonschema2avro")
        .unwrap()
        .arg(input_path.to_str().unwrap())
        .arg(output_path.to_str().unwrap())
        .assert()
        .success();

    // Read output Avro schema
    let output = fs::read_to_string(&output_path).unwrap();
    let json: serde_json::Value = serde_json::from_str(&output).unwrap();

    // Snapshot
    assert_json_snapshot!("basic_string_schema", json);
}
