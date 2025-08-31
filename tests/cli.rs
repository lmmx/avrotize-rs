use assert_cmd::Command;
use tempfile::tempdir;
use std::fs;
use insta::assert_json_snapshot;

#[test]
fn cli_converts_basic_object_schema() {
    let dir = tempdir().unwrap();
    let input_path = dir.path().join("schema.json");
    let output_path = dir.path().join("schema.avsc");

    // Minimal valid JSON Schema
    fs::write(
        &input_path,
        r#"{
            "type": "object",
            "properties": {
                "name": { "type": "string" }
            },
            "required": ["name"]
        }"#,
    ).unwrap();

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
    assert_json_snapshot!("basic_object_schema", json);
}