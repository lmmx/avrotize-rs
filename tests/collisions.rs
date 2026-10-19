//! Regression tests for #28: differently shaped types that share a name, and
//! bookkeeping keys leaking into the output.
use avrotize::converter::jsons_to_avro;
use serde_json::{json, Value};
use std::collections::HashMap;

/// `r` holds arrays of one object shape, a second object shape, or integers (#28).
fn issue_28_schema() -> Value {
    json!({"type": "object", "required": ["r"], "properties": {"r": {"anyOf": [
        {"type": "array", "items": {"anyOf": [
            {"type": "array", "items": {"type": "object", "properties": {
                "d": {"type": "null"}, "__boolean": {"type": ["null", "boolean"]}}}},
            {"type": "object", "properties": {"d": {"type": "integer"}}, "required": ["d"]},
            {"type": "integer"}]}},
        {"type": "boolean"}]}}})
}

/// Every object in `avro` that is a type, skipping `const` and `default` data.
fn types(avro: &Value) -> Vec<&Value> {
    let mut out = Vec::new();
    match avro {
        Value::Object(obj) => {
            if obj.contains_key("type") {
                out.push(avro);
            }
            for (k, v) in obj {
                if k != "const" && k != "default" {
                    out.extend(types(v));
                }
            }
        }
        Value::Array(arr) => arr.iter().for_each(|v| out.extend(types(v))),
        _ => {}
    }
    out
}

fn assert_no_bookkeeping(avro: &Value) {
    for t in types(avro) {
        for key in ["dependencies", "unmerged_types"] {
            assert!(t.get(key).is_none(), "{key} leaked into {t}");
        }
    }
}

/// The named types `avro` defines, by full name, checked the way an Avro parser
/// checks them: no full name is defined twice, and every reference is defined.
fn named_types(avro: &Value) -> HashMap<String, &Value> {
    const PRIMITIVES: [&str; 8] = [
        "null", "boolean", "int", "long", "float", "double", "bytes", "string",
    ];
    let mut defined = HashMap::new();
    let mut referenced = Vec::new();
    for t in types(avro) {
        if let (Some("record" | "enum" | "fixed"), Some(name)) =
            (t["type"].as_str(), t["name"].as_str())
        {
            let full = match t["namespace"].as_str() {
                Some(ns) if !ns.is_empty() => format!("{ns}.{name}"),
                _ => name.to_string(),
            };
            assert!(
                defined.insert(full.clone(), t).is_none(),
                "{full} is defined twice"
            );
        }
        for key in ["type", "items", "values"] {
            let refs = match &t[key] {
                Value::Array(union) => union.iter().collect(),
                other => vec![other],
            };
            referenced.extend(refs.into_iter().filter_map(Value::as_str));
        }
    }
    for name in referenced {
        let is_type_keyword = matches!(name, "record" | "enum" | "fixed" | "array" | "map");
        assert!(
            is_type_keyword || PRIMITIVES.contains(&name) || defined.contains_key(name),
            "{name} is referenced but not defined"
        );
    }
    defined
}

/// Whether the JSON `datum` can be written with the Avro `schema`.
fn fits(schema: &Value, datum: &Value, names: &HashMap<String, &Value>) -> bool {
    match schema {
        Value::String(s) => match s.as_str() {
            "null" => datum.is_null(),
            "boolean" => datum.is_boolean(),
            "int" | "long" => datum.is_i64() || datum.is_u64(),
            "float" | "double" => datum.is_number(),
            "string" | "bytes" => datum.is_string(),
            name => fits(names[name], datum, names),
        },
        Value::Array(union) => branch(union, datum, names).is_some(),
        Value::Object(obj) => match obj["type"].as_str() {
            Some("record") => datum.as_object().is_some_and(|d| {
                let fields = obj["fields"].as_array().unwrap();
                d.keys().all(|k| fields.iter().any(|f| f["name"] == *k))
                    && fields.iter().all(|f| {
                        let value = d.get(f["name"].as_str().unwrap());
                        fits(&f["type"], value.unwrap_or(&Value::Null), names)
                    })
            }),
            Some("array") => datum
                .as_array()
                .is_some_and(|items| items.iter().all(|i| fits(&obj["items"], i, names))),
            Some("map") => datum
                .as_object()
                .is_some_and(|m| m.values().all(|v| fits(&obj["values"], v, names))),
            Some("enum") => datum
                .as_str()
                .is_some_and(|s| obj["symbols"].as_array().unwrap().iter().any(|x| x == s)),
            _ => fits(&obj["type"], datum, names),
        },
        _ => false,
    }
}

/// The first branch of `union` that `datum` can be written with.
fn branch(union: &[Value], datum: &Value, names: &HashMap<String, &Value>) -> Option<usize> {
    union.iter().position(|b| fits(b, datum, names))
}

#[test]
fn differently_shaped_records_get_distinct_names() {
    let avro = jsons_to_avro(&issue_28_schema(), "i238.schema", "", "", false);
    assert_eq!(
        avro,
        json!({"type": "record", "name": "document", "namespace": "i238.schema", "fields": [
            {"name": "r", "type": [
                {"type": "array", "items": [
                    {"type": "array", "items": {
                        "type": "record", "name": "r", "namespace": "i238.schema.document_types",
                        "fields": [
                            {"name": "d", "type": ["null"]},
                            {"name": "__boolean", "type": ["null", "boolean"]}]}},
                    {"type": "record", "name": "r_1", "namespace": "i238.schema.document_types",
                        "fields": [{"name": "d", "type": "int"}]},
                    "int"]},
                "boolean"]}]})
    );
    named_types(&avro);
}

#[test]
fn integer_field_d_9_is_written_with_its_own_record() {
    let avro = jsons_to_avro(&issue_28_schema(), "i238.schema", "", "", false);
    let names = named_types(&avro);
    let r = avro["fields"][0]["type"].as_array().unwrap();
    let items = r[0]["items"].as_array().unwrap();

    // {"r": [{"d": 9}]}: r is the array branch, and {"d": 9} is the r_1 branch, whose d is an int.
    let datum = json!({"d": 9});
    assert_eq!(branch(r, &json!([datum]), &names), Some(0));
    assert_eq!(branch(items, &datum, &names), Some(1));
    assert_eq!(items[1]["name"], "r_1");
    assert_eq!(items[1]["fields"], json!([{"name": "d", "type": "int"}]));

    // The first shape still has a home: an array of the record r.
    let first = json!([{"d": null, "__boolean": true}]);
    assert_eq!(branch(items, &first, &names), Some(0));
    assert_eq!(items[0]["items"]["name"], "r");
    // An integer item is the int branch.
    assert_eq!(branch(items, &json!(9), &names), Some(2));
    assert!(fits(
        &avro,
        &json!({"r": [{"d": 9}, [{"d": null}], 3]}),
        &names
    ));
}

#[test]
fn enums_with_different_symbols_get_distinct_names() {
    let schema = json!({"type": "object", "required": ["a", "b"], "properties": {
        "a": {"type": "object", "required": ["kind"], "properties": {"kind": {"const": "x"}}},
        "b": {"type": "object", "required": ["kind"], "properties": {"kind": {"const": "y"}}}}});
    let avro = jsons_to_avro(&schema, "ns", "", "", false);
    let kind = |i: usize| avro["fields"][i]["type"]["fields"][0]["type"].clone();
    assert_eq!(
        kind(0),
        json!({"name": "kind", "type": "enum", "namespace": "ns.document_types", "symbols": ["x"]})
    );
    assert_eq!(
        kind(1),
        json!({"name": "kind_1", "type": "enum", "namespace": "ns.document_types", "symbols": ["y"]})
    );
    let names = named_types(&avro);
    assert!(fits(
        &avro,
        &json!({"a": {"kind": "x"}, "b": {"kind": "y"}}),
        &names
    ));
}

#[test]
fn identical_records_are_still_defined_once() {
    let record =
        json!({"type": "object", "properties": {"x": {"type": "integer"}}, "required": ["x"]});
    let schema = json!({"type": "object", "required": ["r"], "properties": {"r": {"anyOf": [
        record, {"type": "array", "items": record}]}}});
    let avro = jsons_to_avro(&schema, "ns", "", "", false);
    assert_eq!(
        avro["fields"][0]["type"],
        json!([
            {"type": "record", "name": "r", "namespace": "ns.document_types",
                "fields": [{"name": "x", "type": "int"}]},
            {"type": "array", "items": "ns.document_types.r"}])
    );
    named_types(&avro);
}

#[test]
fn renamed_types_skip_names_already_taken() {
    // `r_1` is already a property name, so the second `r` shape becomes `r_2`.
    let schema = json!({"type": "object", "required": ["r", "r_1"], "properties": {
        "r_1": {"type": "object", "properties": {"z": {"type": "string"}}, "required": ["z"]},
        "r": {"anyOf": [
            {"type": "object", "properties": {"x": {"type": "integer"}}, "required": ["x"]},
            {"type": "array", "items":
                {"type": "object", "properties": {"y": {"type": "boolean"}}, "required": ["y"]}}]}}});
    let avro = jsons_to_avro(&schema, "ns", "", "", false);
    let names: Vec<&str> = types(&avro)
        .into_iter()
        .filter(|t| t["type"] == "record")
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["document", "r_1", "r", "r_2"]);
    named_types(&avro);
}

#[test]
fn no_bookkeeping_keys_in_any_output_mode() {
    let schema = issue_28_schema();
    assert_no_bookkeeping(&jsons_to_avro(&schema, "ns", "", "", false));
    assert_no_bookkeeping(&jsons_to_avro(&schema, "ns", "", "", true));

    let mut with_defs = schema.clone();
    with_defs["$defs"] = json!({"thing": {"type": "object", "properties": {
        "d": {"type": "null"}, "e": {"type": "integer"}}}});
    assert_no_bookkeeping(&jsons_to_avro(&with_defs, "ns", "", "", false));
    assert_no_bookkeeping(&jsons_to_avro(&with_defs, "ns", "", "", true));
}

#[test]
fn const_data_named_like_a_bookkeeping_key_is_kept() {
    let schema = json!({"type": "object", "properties": {
        "c": {"const": {"dependencies": ["kept"]}},
        "n": {"type": "null"}}});
    let avro = jsons_to_avro(&schema, "ns", "", "", false);
    assert_eq!(
        avro["fields"][0]["const"],
        json!({"dependencies": ["kept"]})
    );
    assert_no_bookkeeping(&avro);
}

#[test]
fn every_fixture_defines_each_name_once_without_bookkeeping() {
    let mut checked = 0;
    for entry in std::fs::read_dir("tests/fixtures/jsonschema").unwrap() {
        let path = entry.unwrap().path();
        let stem = path.file_stem().unwrap().to_str().unwrap().to_string();
        if stem == "recursive_ref" {
            continue; // fetches its $ref over the network
        }
        let schema: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let avro = jsons_to_avro(&schema, &stem, "", "", false);
        named_types(&avro);
        assert_no_bookkeeping(&avro);
        checked += 1;
    }
    assert!(checked > 30, "only {checked} fixtures found");
}

#[test]
#[should_panic(expected = "ns.r is defined twice")]
fn checker_rejects_a_name_defined_twice() {
    let r = |field: &str| json!({"type": "record", "name": "r", "namespace": "ns", "fields": [{"name": field, "type": "int"}]});
    named_types(
        &json!({"type": "record", "name": "doc", "namespace": "ns", "fields": [
        {"name": "a", "type": r("x")}, {"name": "b", "type": r("y")}]}),
    );
}

#[test]
#[should_panic(expected = "ns.missing is referenced but not defined")]
fn checker_rejects_an_undefined_reference() {
    named_types(
        &json!({"type": "record", "name": "doc", "namespace": "ns", "fields": [
        {"name": "a", "type": "ns.missing"}]}),
    );
}
