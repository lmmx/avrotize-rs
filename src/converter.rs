pub mod analysis;
pub mod composition;
pub mod conversion;
pub mod definitions;
pub mod emptiness;
pub mod merging;
pub mod postprocess;
pub mod references;
pub mod state;
pub mod structs;
pub mod types;
pub mod unions;
pub mod utils;

pub use state::JsonToAvroConverter;

use definitions::{process_definition, process_definition_list};
use postprocess::postprocess_schema;
use utils::id_to_avro_namespace;

use serde_json::Value;
use std::fs;
use std::path::Path;
use url::Url;

use crate::common::traversal::find_schema_node;
use crate::dependency_resolver::{inline_dependencies_of, sort_messages_by_dependencies};

/// Convert an in-memory JSON Schema into an Avro Schema.
///
/// This handles definitions, root objects, and dependency resolution.
/// Returns either a single Avro schema object or a list of schemas.
pub fn jsons_to_avro(
    json_schema: &Value,
    namespace: &str,
    utility_namespace: &str,
    base_uri: &str,
    split_top_level: bool,
) -> Value {
    let mut avro_schema: Vec<Value> = Vec::new();
    let mut record_stack: Vec<String> = Vec::new();

    let url = Url::parse(base_uri).unwrap_or_else(|_| Url::parse("file:///tmp").unwrap());
    let mut root_name = "document".to_string();
    let mut root_namespace = namespace.to_string();

    // definitions / $defs
    if let Some(defs) = json_schema
        .get("definitions")
        .or_else(|| json_schema.get("$defs"))
    {
        if let Some(map) = defs.as_object() {
            for (def_name, schema) in map {
                if schema.is_object() {
                    process_definition(
                        json_schema,
                        namespace,
                        utility_namespace,
                        base_uri,
                        &mut avro_schema,
                        &mut record_stack,
                        def_name,
                        schema,
                        false,
                    );
                }
            }
        }
    }

    // Root
    if json_schema.is_object() {
        if let Some((ns, name)) = process_definition(
            json_schema,
            namespace,
            utility_namespace,
            base_uri,
            &mut avro_schema,
            &mut record_stack,
            &root_name,
            json_schema,
            true,
        ) {
            root_namespace = ns;
            root_name = name;
        }
    }

    // Postprocess unmerged types
    postprocess_schema(&mut avro_schema, Vec::new());

    // Inline or sort
    if split_top_level {
        Value::Array(
            avro_schema
                .into_iter()
                .filter(|item| item.get("type").and_then(|t| t.as_str()) == Some("record"))
                .collect(),
        )
    } else if !avro_schema.is_empty() {
        if !json_schema.get("definitions").is_some() && !json_schema.get("$defs").is_some() {
            let mut recursion_stack = Vec::new();
            if let Some(root) = find_schema_node(
                &|t: &Value| {
                    t.get("name").and_then(|n| n.as_str()) == Some(&root_name)
                        && t.get("namespace").and_then(|n| n.as_str()) == Some(&root_namespace)
                },
                &Value::Array(avro_schema.clone()),
                &mut recursion_stack,
            ) {
                let mut root_copy = root.clone();
                inline_dependencies_of(&mut avro_schema.clone(), &mut root_copy);
                return root_copy;
            }
        }
        Value::Array(sort_messages_by_dependencies(&mut avro_schema.clone()))
    } else {
        Value::Array(Vec::new())
    }
}

/// Convert JSON Schema file into Avro Schema file(s).
///
/// This reads a JSON Schema file (from disk or HTTP), converts it to Avro,
/// and writes the `.avsc` file(s) to the given path.
///
/// # Arguments
/// * `json_schema_file_path` - Path or URL of the input JSON Schema.
/// * `avro_schema_path` - Path where the Avro schema file(s) will be written.
/// * `namespace` - Optional namespace override.
/// * `utility_namespace` - Optional namespace for utility types.
/// * `root_class_name` - Optional name for the root record type.
/// * `split_top_level_records` - If true, write each top-level record to a separate file.
///
/// # Returns
/// Returns `Ok(())` on success, or an error string if conversion failed.
pub fn convert_jsons_to_avro(
    json_schema_file_path: &str,
    avro_schema_path: &str,
    namespace: Option<&str>,
    utility_namespace: Option<&str>,
    _root_class_name: Option<&str>,
    split_top_level_records: bool,
) -> Result<(), String> {
    let content = if json_schema_file_path.starts_with("http") {
        reqwest::blocking::get(json_schema_file_path)
            .map_err(|e| format!("HTTP fetch failed: {e}"))?
            .text()
            .map_err(|e| format!("Invalid response body: {e}"))?
    } else {
        fs::read_to_string(json_schema_file_path)
            .map_err(|e| format!("Failed to read schema file: {e}"))?
    };

    let json_schema: Value =
        serde_json::from_str(&content).map_err(|e| format!("Invalid JSON schema: {e}"))?;

    let mut ns: String = namespace.map(|s| s.to_string()).unwrap_or_else(|| {
        Path::new(json_schema_file_path)
            .file_stem()
            .unwrap()
            .to_str()
            .unwrap()
            .to_string()
    });

    if let Some(id) = json_schema.get("$id").and_then(|v| v.as_str()) {
        let id_ns = id_to_avro_namespace(id);
        if !id_ns.is_empty() {
            ns = id_ns;
        }
    }

    let utility_ns = if let Some(u) = utility_namespace {
        u.to_string()
    } else {
        format!("{ns}.utility")
    };
    // dbg!(&ns, &utility_ns);

    let avro_schema = jsons_to_avro(
        &json_schema,
        &ns,
        &utility_ns,
        json_schema_file_path,
        split_top_level_records,
    );

    if split_top_level_records {
        if let Some(arr) = avro_schema.as_array() {
            for item in arr {
                if let Some(name) = item.get("name").and_then(|n| n.as_str()) {
                    let file_path = Path::new(avro_schema_path).join(format!("{name}.avsc"));
                    fs::write(&file_path, serde_json::to_string_pretty(item).unwrap())
                        .map_err(|e| format!("Failed to write {file_path:?}: {e}"))?;
                }
            }
        }
    } else {
        fs::write(
            avro_schema_path,
            serde_json::to_string_pretty(&avro_schema).unwrap(),
        )
        .map_err(|e| format!("Failed to write {avro_schema_path}: {e}"))?;
    }

    Ok(())
}
