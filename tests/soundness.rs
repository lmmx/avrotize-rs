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

/// A JSON Schema with one required property `p` and the given `$defs`.
fn with_defs(p: Value, defs: Value) -> Value {
    json!({"type": "object", "required": ["p"], "properties": {"p": p}, "$defs": defs})
}

/// The type of property `p` of the `document` record in a list of schemas.
fn p_type(avro: &Value) -> &Value {
    let document = avro
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "document")
        .unwrap();
    &document["fields"][0]["type"]
}

#[test]
fn a_ref_to_a_primitive_definition_takes_its_type() {
    // Was ns.Str, which nothing defines: a primitive definition is not a named type.
    let avro = convert(&with_defs(
        json!({"$ref": "#/$defs/Str"}),
        json!({"Str": {"type": "string"}}),
    ));
    assert_eq!(p_type(&avro), &json!("string"));
}

#[test]
fn a_nested_ref_names_the_definition_in_its_own_namespace() {
    // Was ns.document_types.Rec: the nested record's namespace, not the definition's.
    let rec =
        json!({"type": "object", "required": ["x"], "properties": {"x": {"type": "integer"}}});
    let schema = with_defs(
        json!({"type": "object", "required": ["q"], "properties": {"q": {"$ref": "#/$defs/Rec"}}}),
        json!({"Rec": rec}),
    );
    let avro = convert(&schema);
    assert_eq!(p_type(&avro)["fields"][0]["type"], json!("ns.Rec"));
}

#[test]
fn a_ref_uses_the_name_the_definition_was_given() {
    // Was ns.my-rec; the definition is registered as my_rec.
    let avro = convert(&with_defs(
        json!({"$ref": "#/$defs/my-rec"}),
        json!({"my-rec": {"type": "object", "properties": {"x": {"type": "integer"}}}}),
    ));
    assert_eq!(p_type(&avro), &json!("ns.my_rec"));
}

#[test]
fn a_definition_comes_before_the_definitions_that_use_it() {
    // A refers to B, but A came first: a schema list must define a name before use.
    let int = json!({"type": "integer"});
    let avro = convert(&with_defs(
        json!({"$ref": "#/$defs/A"}),
        json!({
            "A": {"type": "object", "properties": {"b": {"$ref": "#/$defs/B"}}},
            "B": {"type": "object", "properties": {"x": int}}}),
    ));
    let names: Vec<&str> = avro
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["B", "A", "document"]);
}

#[test]
fn refs_in_items_and_union_branches_keep_their_type() {
    // Each became the generic "any" union.
    let rec = json!({"Rec": {"type": "object", "properties": {"x": {"type": "integer"}}}});
    let items = convert(&with_defs(
        json!({"type": "array", "items": {"$ref": "#/$defs/Rec"}}),
        rec.clone(),
    ));
    assert_eq!(p_type(&items), &json!({"type": "array", "items": "ns.Rec"}));
    let branch = convert(&with_defs(
        json!({"anyOf": [{"$ref": "#/$defs/Rec"}, {"type": "null"}]}),
        rec,
    ));
    assert_eq!(p_type(&branch), &json!(["ns.Rec", "null"]));
}

#[test]
fn defs_are_used_alongside_definitions() {
    // Only `definitions` was read when both were present, so $defs refs dangled.
    let schema = json!({"type": "object", "required": ["p", "q"], "properties": {
            "p": {"$ref": "#/$defs/A"}, "q": {"$ref": "#/definitions/B"}},
        "$defs": {"A": {"type": "object", "properties": {"a": {"type": "integer"}}}},
        "definitions": {"B": {"type": "object", "properties": {"b": {"type": "integer"}}}}});
    let avro = convert(&schema);
    assert_eq!(p_type(&avro), &json!("ns.A"));
}

#[test]
fn recursive_definitions_refs_become_named_references() {
    // A definitions entry referring to itself was expanded inline without end, which
    // overflowed the stack and aborted the process.
    let node = json!({"type": "object", "required": ["v"], "properties": {
        "v": {"type": "integer"}, "next": {"$ref": "#/definitions/Node"}}});
    let schema = json!({"type": "object", "required": ["p"],
        "properties": {"p": {"$ref": "#/definitions/Node"}}, "definitions": {"Node": node}});
    let avro = convert(&schema);
    let node = &avro[0];
    assert_eq!(node["name"], "Node");
    assert_eq!(node["fields"][1]["type"], json!(["null", "ns.Node"]));
    assert_fits(
        node,
        json!({"v": 1, "next": {"v": 2, "next": {"v": 3, "next": null}}}),
    );
}

#[test]
fn a_tree_and_mutually_recursive_definitions_convert() {
    let tree = json!({"type": "object", "properties": {
        "l": {"$ref": "#/definitions/T"}, "r": {"$ref": "#/definitions/T"}}});
    let a = json!({"type": "object", "properties": {"b": {"$ref": "#/definitions/B"}}});
    let b = json!({"type": "object", "properties": {"a": {"$ref": "#/definitions/A"}}});
    let schema = json!({"type": "object", "properties": {
            "t": {"$ref": "#/definitions/T"}, "a": {"$ref": "#/definitions/A"}},
        "definitions": {"T": tree, "A": a, "B": b}});
    let avro = convert(&schema);
    let t = avro
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["name"] == "T")
        .unwrap();
    assert_eq!(t["fields"][0]["type"], json!(["null", "ns.T"]));
    assert_fits(t, json!({"l": {"l": null, "r": null}, "r": null}));
}

#[test]
fn a_ref_to_the_root_converts() {
    // `#` is the whole schema: a reference to it inside it recursed without end too.
    let schema = json!({"type": "object", "properties": {"p": {"$ref": "#"}}});
    convert(&schema);
}

#[test]
fn a_pattern_map_field_does_not_take_a_property_name() {
    // The patternProperties map is a field named after its record, so a record `kind`
    // with a property `kind` had two fields named kind.
    let schema = json!({"type": "object", "required": ["kind"], "properties": {"kind": {
        "type": "object", "required": ["kind"],
        "properties": {"kind": {"type": "string"}},
        "patternProperties": {"^p": {"type": "integer"}}}}});
    let avro = convert(&schema);
    let fields: Vec<&Value> = avro["fields"][0]["type"]["fields"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| &f["name"])
        .collect();
    assert_eq!(fields, [&json!("kind"), &json!("kind_1")]);
}

#[test]
fn data_shaped_like_a_type_is_not_renamed() {
    // Renaming walked into const data, so a const object shaped like a record r was
    // renamed to r_1 when a different record r existed: the constant's value changed.
    // In the root namespace, the dependency resolver then replaced it with "ns...r".
    let record_like =
        json!({"type": "record", "name": "r", "namespace": "ns.document_types", "fields": []});
    let schema = json!({"type": "object", "properties": {
        "r": {"type": "object", "properties": {"x": {"type": "integer"}}},
        "k": {"const": record_like.clone()},
        "l": {"const": [1, 1, 2]}}});
    let avro = convert(&schema);
    assert_eq!(avro["fields"][1]["const"], record_like);
    // The dependency resolver also replaced repeated record-like data with a reference,
    // and deduplicated adjacent array items in data: [1, 1, 2] became [1, 2].
    assert_eq!(avro["fields"][2]["const"], json!([1, 1, 2]));
}

#[test]
fn a_nullable_type_list_keeps_null_where_it_is_required() {
    // ["boolean", "null"] lost its null unless the field was optional, which added one
    // back: a required field, an array item or a union branch couldn't hold null.
    let schema = json!({"type": "object", "required": ["n", "m", "a", "u"], "properties": {
        "n": {"type": ["boolean", "null"]},
        "m": {"type": ["string", "integer", "null"]},
        "a": {"type": "array", "items": {"type": ["string", "null"]}},
        "u": {"anyOf": [{"type": ["boolean", "null"]}, {"type": "string"}]},
        "o": {"type": ["null", "integer"]}}});
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
            &json!(["null", "boolean"]),
            &json!(["null", "string", "int"]),
            &json!({"type": "array", "items": ["null", "string"]}),
            &json!(["null", "boolean", "string"]),
            // An optional field already gained null this way: unchanged.
            &json!(["null", "int"]),
        ]
    );
    assert_fits(
        &avro,
        json!({"n": null, "m": null, "a": ["x", null], "u": null, "o": null}),
    );
}

#[test]
fn a_union_pattern_type_is_not_nested_in_the_map_values() {
    let schema = json!({"type": "object", "patternProperties": {
        "^p": {"type": ["string", "null"]}, "^q": {"anyOf": [{"type": "integer"}, {"type": "boolean"}]}}});
    let avro = convert(&schema);
    assert_eq!(
        avro["fields"][0]["type"]["values"],
        json!(["null", "string", "int", "boolean"])
    );
}

#[test]
fn additional_properties_true_holds_values_of_any_type() {
    // Was a map of strings, so a number or an object among the extra keys didn't fit.
    let schema = json!({"type": "object", "required": ["k"], "properties": {
        "k": {"type": "object", "properties": {}, "additionalProperties": true}}});
    let avro = convert(&schema);
    assert_eq!(avro["fields"][0]["type"]["type"], "map");
    assert_fits(
        &avro,
        json!({"k": {"s": "x", "n": 1, "o": {"a": true}, "l": [1, "y"]}}),
    );
}

#[test]
fn an_object_without_properties_holds_any_properties() {
    // #12: such an object became a record with no fields, which drops every key.
    let schema = json!({"type": "object", "required": ["m", "n", "a"], "properties": {
        "m": {"type": "object"},
        "n": {"type": "object", "maxProperties": 3},
        "a": {"type": "array", "items": {"type": "object", "minProperties": 1}}}});
    let avro = convert(&schema);
    for field in avro["fields"].as_array().unwrap() {
        let t = &field["type"];
        let map = if t["type"] == "array" { &t["items"] } else { t };
        assert_eq!(map["type"], "map", "{field}");
    }
    assert_fits(
        &avro,
        json!({"m": {"k": 1}, "n": {"x": "y"}, "a": [{"z": [true]}]}),
    );
    // The root too: a wrapper record holding the map.
    let root = convert(&json!({"type": "object", "maxProperties": 3}));
    assert_eq!(root["fields"][0]["type"]["type"], "map");
}

#[test]
fn an_object_that_must_be_empty_or_declares_properties_stays_a_record() {
    let schema = json!({"type": "object", "required": ["f", "e"], "properties": {
        "f": {"type": "object", "additionalProperties": false},
        "e": {"type": "object", "properties": {}}}});
    let avro = convert(&schema);
    for field in avro["fields"].as_array().unwrap() {
        assert_eq!(field["type"]["type"], "record", "{field}");
    }
}

#[test]
fn a_union_stays_well_formed_after_a_definition_is_inlined_into_it() {
    // The union was normalized before the reference to Str became "string", which
    // then repeated the string already there.
    let defs = json!({"Str": {"type": "string"}});
    let avro = convert(&with_defs(
        json!({"anyOf": [{"$ref": "#/$defs/Str"}, {"type": "string"}, {"type": "integer"}]}),
        defs.clone(),
    ));
    assert_eq!(p_type(&avro), &json!(["string", "int"]));
    convert(&with_defs(
        json!({"anyOf": [{"$ref": "#/$defs/Str"}, {}]}),
        defs,
    ));
}

#[test]
fn date_time_keeps_the_time_and_duration_is_a_named_fixed_type() {
    // date-time was a date (an int of days), dropping the time; duration was a fixed
    // type without a name, which strict parsers reject.
    let schema = json!({"type": "object", "required": ["at", "d1", "d2"], "properties": {
        "at": {"type": "string", "format": "date-time"},
        "d1": {"type": "string", "format": "duration"},
        "d2": {"type": "string", "format": "duration"}}});
    let avro = convert(&schema);
    let fields = avro["fields"].as_array().unwrap();
    assert_eq!(
        fields[0]["type"],
        json!({"type": "long", "logicalType": "timestamp-millis"})
    );
    assert_eq!(
        fields[1]["type"],
        json!({"type": "fixed", "name": "duration", "namespace": "ns", "size": 12,
            "logicalType": "duration"})
    );
    // The second identical duration refers to the first.
    assert_eq!(fields[2]["type"], json!("ns.duration"));
}
