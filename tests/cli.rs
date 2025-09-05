#![cfg(feature = "cli")]
use assert_cmd::Command;
use insta::{assert_json_snapshot, assert_snapshot};
use rstest::rstest;
use serde_json::Value;
use similar::{ChangeTag, TextDiff};
use std::{fs, path::Path};
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
#[case("object_with_defs")]
#[case("object_with_const_field")]
#[case("object_with_default_value")]
#[case("object_with_explicit_nullable_type")]
#[case("object_with_enum_array")]
//#[case("object_with_map_via_additional_props")]
//#[case("object_with_oneof_anyof")]
// New fixtures:
#[case("array_contains")]
// #[case("array_maxitems")]
#[case("array_uniqueitems")]
// #[case("array_with_additional_items")]
#[case("boolean_false_schema")]
#[case("boolean_true_schema")]
#[case("empty_schema")]
// #[case("number_exclusive_max")]
// #[case("number_exclusive_min")]
// #[case("number_exclusive_multipleof")]
// #[case("number_with_maximum")]
// #[case("object_allof")]
#[case("object_dependentrequired")]
#[case("object_dependentschemas")]
#[case("object_if_then_else")]
// #[case("object_maxproperties")]
// #[case("object_minproperties")]
// #[case("object_not")]
// #[case("object_pattern_properties")]
#[case("object_with_remote_ref")]
// #[case("recursive_ref")]
#[case("string_format_email")]
// #[case("string_maxlength")]
// #[case("string_minlength")]
fn cli_fixtures(#[case] stem: &str) {
    let schema_path = format!("tests/fixtures/jsonschema/{stem}.json");
    run_fixture(&schema_path, stem);
}

fn normalize_json(input: &str) -> String {
    let value: Value = serde_json::from_str(input).unwrap();
    serde_json::to_string_pretty(&value).unwrap()
}

fn diff_fixture(stem: &str) -> Option<String> {
    let fixture_path = format!("tests/fixtures/avro/{stem}.avsc");
    let snap_path = format!("tests/snapshots/cli__{stem}.snap");

    if !Path::new(&fixture_path).exists() || !Path::new(&snap_path).exists() {
        return None;
    }

    let fixture_raw = fs::read_to_string(fixture_path).unwrap();
    let snapshot_raw = fs::read_to_string(snap_path).unwrap();
    let snapshot_stripped = snapshot_raw
        .lines()
        .skip_while(|line| !line.starts_with('{') && !line.starts_with('['))
        .collect::<Vec<_>>()
        .join("\n");

    let fixture = normalize_json(&fixture_raw);
    let snapshot = normalize_json(&snapshot_stripped);

    let diff = TextDiff::from_lines(&fixture, &snapshot);

    if diff.ratio() == 1.0 {
        // No syntactic changes: a perfect match with avrotize's output
        return Some(String::new());
    }

    let mut buf = String::new();
    for op in diff.ops() {
        for change in diff.iter_inline_changes(op) {
            let sign = match change.tag() {
                ChangeTag::Delete => "-",
                ChangeTag::Insert => "+",
                ChangeTag::Equal => " ",
            };
            buf.push_str(sign);

            for (emphasized, value) in change.values() {
                if *emphasized {
                    buf.push_str(&format!("[[{value}]]"));
                } else {
                    buf.push_str(value);
                }
            }
            buf.push('\n');
        }
    }

    Some(buf)
}

#[rstest]
#[case("array_contains")]
#[case("array_of_objects")]
#[case("array_uniqueitems")]
#[case("basic_string_schema")]
#[case("basic_string_schema_with_title")]
#[case("empty_schema")]
#[case("enum_string_property")]
#[case("nested_object_and_array")]
#[case("object_dependentrequired")]
#[case("object_dependentschemas")]
#[case("object_if_then_else")]
#[case("object_with_boolean_and_number")]
#[case("object_with_const_field")]
#[case("object_with_default_value")]
#[case("object_with_defs")]
#[case("object_with_enum_array")]
#[case("object_with_explicit_nullable_type")]
#[case("object_with_optional")]
#[case("string_format_email")]
fn diff_snapshots(#[case] stem: &str) {
    if let Some(diff) = diff_fixture(stem) {
        assert_snapshot!(format!("{stem}.diff"), diff);
    }
}
