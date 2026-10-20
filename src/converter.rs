//! Conversion from JSON Schema to Avro schema.
//!
//! Provides the main entry points [`jsons_to_avro`] and
//! [`convert_jsons_to_avro`], which can be used programmatically
//! or through the CLI.
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

use definitions::process_definition;
use postprocess::{disambiguate_named_types, postprocess_schema, strip_bookkeeping};
use utils::id_to_avro_namespace;

use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use url::Url;

use crate::common::traversal::find_schema_node;
use crate::dependency_resolver::{inline_dependencies_of, sort_messages_by_dependencies};

#[cfg_attr(feature = "trace", crustrace::instrument)]
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

    let _url = Url::parse(base_uri).unwrap_or_else(|_| Url::parse("file:///tmp").unwrap());
    let mut root_name = "document".to_string();
    let mut root_namespace = namespace.to_string();

    // definitions / $defs, remembering the name each is registered under
    let mut registered: HashMap<String, String> = HashMap::new();
    for container in ["definitions", "$defs"] {
        let Some(map) = json_schema.get(container).and_then(|d| d.as_object()) else {
            continue;
        };
        for (def_name, schema) in map {
            if schema.is_object() {
                // A definition that refers to itself gets a reference to its own name.
                let ref_str = format!("#/{container}/{def_name}");
                let defined = conversion::expand_once(&ref_str, || {
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
                    )
                });
                if let Some(Some((ns, name))) = defined {
                    registered.insert(ref_str, format!("{ns}.{name}"));
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

    // Point references to definitions at what each definition became
    resolve_definition_refs(
        &mut avro_schema,
        &registered,
        json_schema,
        namespace,
        utility_namespace,
        base_uri,
    );

    // Give differently shaped types that share a full name distinct names
    disambiguate_named_types(&mut avro_schema);

    // Postprocess unmerged types
    postprocess_schema(&mut avro_schema, Vec::new());

    // Inline or sort
    let mut avro = if split_top_level {
        Value::Array(
            avro_schema
                .into_iter()
                .filter(|item| item.get("type").and_then(|t| t.as_str()) == Some("record"))
                .collect(),
        )
    } else if !avro_schema.is_empty() {
        if json_schema.get("definitions").is_none() && json_schema.get("$defs").is_none() {
            let is_root = |t: &Value| {
                t.get("name").and_then(|n| n.as_str()) == Some(&root_name)
                    && t.get("namespace").and_then(|n| n.as_str()) == Some(&root_namespace)
            };
            let root = if avro_schema
                .first()
                .is_some_and(|t| is_root(t) && !depends_on_itself(t))
            {
                // The search below would return a copy of the first item without looking
                // further, and inlining never looks the root up, so move it out instead.
                Some(avro_schema.remove(0))
            } else {
                let mut recursion_stack = Vec::new();
                // Search the list in place: it is moved into an array value and back.
                let all = Value::Array(std::mem::take(&mut avro_schema));
                let root = find_schema_node(&is_root, &all, &mut recursion_stack);
                let Value::Array(items) = all else {
                    unreachable!("moved in as an array")
                };
                avro_schema = items;
                root
            };
            if let Some(mut root_copy) = root {
                inline_dependencies_of(&mut avro_schema, &mut root_copy);
                strip_bookkeeping(&mut root_copy);
                return root_copy;
            }
        }
        Value::Array(define_before_use(sort_messages_by_dependencies(
            &mut avro_schema,
        )))
    } else {
        Value::Array(Vec::new())
    };
    strip_bookkeeping(&mut avro);
    avro
}

/// Replace the references to definitions that conversion leaves in type positions: every
/// reference to a `$defs` entry, and a `definitions` entry's reference to itself.
///
/// A definition registered as a named type is referred to by its full name. A
/// definition that is not a named type (a primitive, an array, an empty object) has
/// its type converted once and inlined where it is referred to; a cycle among such
/// definitions falls back to the generic type. A reference to a missing entry becomes
/// the name it had before, which is not defined.
fn resolve_definition_refs(
    avro_schema: &mut [Value],
    registered: &HashMap<String, String>,
    json_schema: &Value,
    namespace: &str,
    utility_namespace: &str,
    base_uri: &str,
) {
    use conversion::{is_any_definition_ref as is_definition_ref, json_type_to_avro_type};
    use unions::union_members;

    /// The `$defs` references in `value`'s type positions (not in const or default data).
    fn collect(value: &Value, refs: &mut HashSet<String>) {
        match value {
            Value::String(r) if is_definition_ref(r) => {
                refs.insert(r.clone());
            }
            Value::Object(obj) => obj
                .iter()
                .filter(|(k, _)| *k != "const" && *k != "default")
                .for_each(|(_, v)| collect(v, refs)),
            Value::Array(arr) => arr.iter().for_each(|v| collect(v, refs)),
            _ => {}
        }
    }

    struct Resolver<'a> {
        registered: &'a HashMap<String, String>,
        /// Converted types of the referenced definitions that are not named types.
        inline: HashMap<String, Value>,
        namespace: &'a str,
        /// Inline definitions being expanded, to stop at a cycle.
        visiting: HashSet<String>,
    }

    impl Resolver<'_> {
        /// The type `ref_str` stands for.
        fn resolve(&mut self, ref_str: &str) -> Value {
            if let Some(full_name) = self.registered.get(ref_str) {
                return Value::String(full_name.clone());
            }
            let Some(t) = self.inline.get(ref_str).cloned() else {
                let name = ref_str.rsplit('/').next().unwrap_or_default();
                return Value::String(format!("{}.{}", self.namespace, name));
            };
            if !self.visiting.insert(ref_str.to_string()) {
                return Value::Array(crate::common::generic_type());
            }
            let mut t = t;
            self.replace(&mut t);
            self.visiting.remove(ref_str);
            t
        }

        fn replace(&mut self, value: &mut Value) {
            match value {
                Value::Object(obj) => {
                    for (k, v) in obj.iter_mut() {
                        if k == "const" || k == "default" {
                            continue;
                        }
                        match v.as_str() {
                            Some(r) if is_definition_ref(r) => *v = self.resolve(r),
                            _ => self.replace(v),
                        }
                    }
                }
                Value::Array(arr) => {
                    let mut resolved_member = false;
                    for v in arr.iter_mut() {
                        match v.as_str() {
                            Some(r) if is_definition_ref(r) => {
                                *v = self.resolve(r);
                                resolved_member = true;
                            }
                            _ => self.replace(v),
                        }
                    }
                    // A reference is only ever a union's member here, and what it became
                    // may repeat another member or be a union itself.
                    if resolved_member {
                        *arr = union_members(std::mem::take(arr));
                    }
                }
                _ => {}
            }
        }
    }

    let mut refs = HashSet::new();
    avro_schema.iter().for_each(|t| collect(t, &mut refs));
    if refs.is_empty() {
        return;
    }

    // Convert the referenced definitions that are not named types, and what they refer to.
    let mut inline = HashMap::new();
    let mut pending: Vec<String> = refs.into_iter().collect();
    pending.sort_unstable();
    while let Some(ref_str) = pending.pop() {
        if registered.contains_key(&ref_str) || inline.contains_key(&ref_str) {
            continue;
        }
        let Some(schema) = ref_str
            .strip_prefix('#')
            .and_then(|p| json_schema.pointer(p))
        else {
            continue;
        };
        let name = ref_str.rsplit('/').next().unwrap_or_default();
        let t = json_type_to_avro_type(
            schema,
            name,
            "",
            namespace,
            utility_namespace,
            &mut Vec::new(),
            json_schema,
            base_uri,
            &mut Vec::new(),
            &mut Vec::new(),
            1,
        );
        let mut more = HashSet::new();
        collect(&t, &mut more);
        pending.extend(more);
        inline.insert(ref_str, t);
    }

    let mut resolver = Resolver {
        registered,
        inline,
        namespace,
        visiting: HashSet::new(),
    };
    for item in avro_schema.iter_mut() {
        resolver.replace(item);
    }
}

/// `items` reordered so that each named item comes before the items that refer to it.
///
/// A schema list is read in order, so a name must be defined before it is used, but a
/// reference to a definition is not one of the dependencies the sort orders by. The
/// earliest item whose references are all defined goes next (the first left, in a
/// cycle), so a list already in a valid order keeps it.
fn define_before_use(items: Vec<Value>) -> Vec<Value> {
    fn full_name(item: &Value) -> Option<String> {
        let name = item.get("name")?.as_str()?;
        Some(match item.get("namespace").and_then(Value::as_str) {
            Some(ns) if !ns.is_empty() => format!("{ns}.{name}"),
            _ => name.to_string(),
        })
    }
    fn refs(value: &Value, names: &HashSet<String>, out: &mut HashSet<String>) {
        match value {
            Value::String(s) if names.contains(s) => {
                out.insert(s.clone());
            }
            Value::Object(obj) => obj
                .iter()
                .filter(|(k, _)| *k != "const" && *k != "default" && *k != "name")
                .for_each(|(_, v)| refs(v, names, out)),
            Value::Array(arr) => arr.iter().for_each(|v| refs(v, names, out)),
            _ => {}
        }
    }

    let names: HashSet<String> = items.iter().filter_map(full_name).collect();
    let mut pending: Vec<(Value, HashSet<String>)> = items
        .into_iter()
        .map(|item| {
            let mut needed = HashSet::new();
            refs(&item, &names, &mut needed);
            if let Some(own) = full_name(&item) {
                needed.remove(&own);
            }
            (item, needed)
        })
        .collect();
    let mut defined = HashSet::new();
    let mut ordered = Vec::with_capacity(pending.len());
    while !pending.is_empty() {
        let next = pending
            .iter()
            .position(|(_, needed)| needed.is_subset(&defined))
            .unwrap_or(0);
        let (item, _) = pending.remove(next);
        if let Some(name) = full_name(&item) {
            defined.insert(name);
        }
        ordered.push(item);
    }
    ordered
}

/// Whether `record` lists itself among its dependencies, by name or full name, as
/// `inline_dependencies_of` would look it up.
fn depends_on_itself(record: &Value) -> bool {
    let Some(deps) = record.get("dependencies").and_then(|d| d.as_array()) else {
        return false;
    };
    let name = record.get("name").and_then(|n| n.as_str());
    let full_name = record
        .get("namespace")
        .and_then(|n| n.as_str())
        .zip(name)
        .map(|(ns, name)| format!("{ns}.{name}"));
    deps.iter()
        .filter_map(|d| d.as_str())
        .any(|d| Some(d) == name || Some(d) == full_name.as_deref())
}

#[cfg_attr(feature = "trace", crustrace::instrument)]
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
