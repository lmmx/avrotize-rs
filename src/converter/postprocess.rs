use serde_json::Value;
use std::collections::{HashMap, HashSet};

use crate::common::traversal::{find_schema_node, set_schema_node};
use crate::converter::analysis::is_standalone_avro_type;
use crate::converter::merging::merge_avro_schemas;
use crate::converter::utils::lift_dependencies_from_type;

/// Finalize an Avro type after construction.
///
/// If the type is a dict whose `"type"` is not a complex type,
/// just return the inner type and push dependencies upward.
pub fn post_check_avro_type(dependencies: &mut Vec<String>, avro_type: Value) -> Value {
    if let Some(obj) = avro_type.as_object() {
        if let Some(Value::String(t)) = obj.get("type") {
            if !matches!(t.as_str(), "array" | "map" | "record" | "enum" | "fixed") {
                let mut new_deps = Vec::new();
                let mut copy = avro_type.clone();
                lift_dependencies_from_type(&mut copy, &mut new_deps);
                dependencies.extend(new_deps);
                return Value::String(t.clone());
            }
        }
    }
    avro_type
}

/// Register a type in the Avro schema list.
///
/// Ensures no duplicate types by name + namespace.
/// Returns true if the type was added.
pub fn register_type(avro_schema: &mut Vec<Value>, avro_type: Value) -> bool {
    let name = avro_type.get("name").and_then(|n| n.as_str()).unwrap_or("");
    let namespace = avro_type
        .get("namespace")
        .and_then(|n| n.as_str())
        .unwrap_or("");

    let exists = avro_schema.iter().any(|t| {
        t.get("name").and_then(|n| n.as_str()) == Some(name)
            && t.get("namespace").and_then(|n| n.as_str()) == Some(namespace)
    });

    if !exists && !avro_type.is_null() && is_standalone_avro_type(&avro_type) {
        avro_schema.push(avro_type);
        return true;
    }
    exists
}

/// Perform a second pass to resolve "unmerged_types" fields.
///
/// This reconciles placeholder union/anyOf types into merged Avro forms.
pub fn postprocess_schema(avro_schema: &mut [Value], types_with_unmerged: Vec<Value>) {
    for ref_type in types_with_unmerged {
        let name = ref_type.get("name").and_then(|n| n.as_str()).unwrap_or("");
        let namespace = ref_type
            .get("namespace")
            .and_then(|n| n.as_str())
            .unwrap_or("");

        // find matching type in the schema
        let mut recursion_stack = Vec::new();
        let found = find_schema_node(
            &|t: &Value| {
                t.get("name").and_then(|n| n.as_str()) == Some(name)
                    && t.get("namespace").and_then(|n| n.as_str()) == Some(namespace)
            },
            &Value::Array(avro_schema.to_vec()),
            &mut recursion_stack,
        );

        if let Some(found_type) = found {
            let unmerged = found_type
                .get("unmerged_types")
                .and_then(|u| u.as_array())
                .cloned()
                .unwrap_or_default();

            if !unmerged.is_empty() {
                let mut base = found_type.clone();
                if let Some(obj) = base.as_object_mut() {
                    obj.remove("unmerged_types");
                }

                let mut deps = Vec::new();
                lift_dependencies_from_type(&mut base, &mut deps);

                let mut mergeable = vec![base];
                mergeable.extend(unmerged);

                let merged = merge_avro_schemas(&mergeable, &[], Some(name), &mut deps);

                let _recursion_stack: Vec<*const Value> = Vec::new();
                set_schema_node(
                    &|t: &Value| {
                        t.get("name").and_then(|n| n.as_str()) == Some(name)
                            && t.get("namespace").and_then(|n| n.as_str()) == Some(namespace)
                    },
                    &merged,
                    &mut Value::Array(avro_schema.to_vec()),
                );
            }
        }
    }
}

/// Keys the converter keeps on types for its own use, which are not Avro attributes.
const BOOKKEEPING_KEYS: [&str; 2] = ["dependencies", "unmerged_types"];

/// The `(namespace, name)` of a named Avro type (record, enum or fixed).
fn named_type_key(avro_type: &Value) -> Option<(String, String)> {
    let obj = avro_type.as_object()?;
    let ty = obj.get("type")?.as_str()?;
    if !matches!(ty, "record" | "enum" | "fixed") {
        return None;
    }
    let name = obj.get("name")?.as_str()?;
    let namespace = obj.get("namespace").and_then(|n| n.as_str()).unwrap_or("");
    Some((namespace.to_string(), name.to_string()))
}

/// The shape of a JSON value, with each child replaced by its interned id.
///
/// Objects list their entries sorted by key, as `Value` equality ignores key order.
#[derive(PartialEq, Eq, Hash)]
enum Shape<'a> {
    Null,
    Bool(bool),
    Number(&'a serde_json::Number),
    String(&'a str),
    Array(Vec<u32>),
    Object(Vec<(&'a str, u32)>),
}

/// Interns shapes, so that two values get the same id exactly when they are equal
/// once bookkeeping keys are stripped as [`strip_bookkeeping`] strips them.
///
/// A shape holds its children's ids, so interning a value costs work proportional to
/// its own entries, and equal ids mean equal values: the map compares whole shapes,
/// and a hash only picks the bucket.
#[derive(Default)]
struct ShapeIds<'a> {
    ids: HashMap<Shape<'a>, u32>,
}

impl<'a> ShapeIds<'a> {
    fn intern(&mut self, shape: Shape<'a>) -> u32 {
        let next = self.ids.len() as u32;
        *self.ids.entry(shape).or_insert(next)
    }

    /// The id of `value`; `data` is set inside `const` and `default` values, which
    /// keep their bookkeeping-named keys.
    fn id(&mut self, value: &'a Value, data: bool) -> u32 {
        let shape = match value {
            Value::Null => Shape::Null,
            Value::Bool(b) => Shape::Bool(*b),
            Value::Number(n) => Shape::Number(n),
            Value::String(s) => Shape::String(s),
            Value::Array(arr) => Shape::Array(arr.iter().map(|v| self.id(v, data)).collect()),
            Value::Object(obj) => {
                let strip = !data && obj.contains_key("type");
                let mut entries: Vec<(&str, u32)> = obj
                    .iter()
                    .filter(|(k, _)| !(strip && BOOKKEEPING_KEYS.contains(&k.as_str())))
                    .map(|(k, v)| {
                        (
                            k.as_str(),
                            self.id(v, data || k == "const" || k == "default"),
                        )
                    })
                    .collect();
                entries.sort_unstable_by(|a, b| a.0.cmp(b.0));
                Shape::Object(entries)
            }
        };
        self.intern(shape)
    }
}

/// Give each differently shaped named type its own name.
///
/// Types are named after the property they came from, so two differently shaped
/// objects under the same key and namespace get the same full name, and dependency
/// resolution would turn the second into a reference to the first, losing its fields.
///
/// In document order, the first definition of a full name keeps it. A later identical
/// definition keeps it too (and is later emitted as a reference), while a later
/// different one takes the name of an earlier identical renamed one or else the first
/// free ordinal suffix (`r_1`, `r_2`, ...).
pub fn disambiguate_named_types(avro_schema: &mut [Value]) {
    /// Collect the full names taken, and count the named types.
    fn collect_names(value: &Value, taken: &mut HashSet<(String, String)>, count: &mut usize) {
        if let Some(key) = named_type_key(value) {
            taken.insert(key);
            *count += 1;
        }
        match value {
            Value::Object(obj) => obj.values().for_each(|v| collect_names(v, taken, count)),
            Value::Array(arr) => arr.iter().for_each(|v| collect_names(v, taken, count)),
            _ => {}
        }
    }

    /// Visit every value in document order, recording for each named type the id of
    /// its shape (as found, before any renaming).
    fn survey<'a>(
        value: &'a Value,
        data: bool,
        ids: &mut ShapeIds<'a>,
        named: &mut Vec<u32>,
    ) -> u32 {
        let slot = named_type_key(value).map(|_| {
            named.push(0);
            named.len() - 1
        });
        let id = match value {
            Value::Array(arr) => {
                let items = arr.iter().map(|v| survey(v, data, ids, named)).collect();
                ids.intern(Shape::Array(items))
            }
            Value::Object(obj) => {
                let strip = !data && obj.contains_key("type");
                let mut entries: Vec<(&str, u32)> = Vec::with_capacity(obj.len());
                for (k, v) in obj {
                    let child_data = data || k == "const" || k == "default";
                    let id = survey(v, child_data, ids, named);
                    if !(strip && BOOKKEEPING_KEYS.contains(&k.as_str())) {
                        entries.push((k.as_str(), id));
                    }
                }
                entries.sort_unstable_by(|a, b| a.0.cmp(b.0));
                ids.intern(Shape::Object(entries))
            }
            _ => ids.id(value, data),
        };
        if let Some(slot) = slot {
            // A named type is compared with its own bookkeeping stripped, even inside
            // const or default data.
            named[slot] = if data { ids.id(value, false) } else { id };
        }
        id
    }

    /// Names given so far, per original full name: (shape id, name given).
    type Given = HashMap<(String, String), Vec<(u32, String)>>;

    fn rename(
        avro_type: &mut Value,
        named: &mut std::slice::Iter<u32>,
        given: &mut Given,
        taken: &mut HashSet<(String, String)>,
    ) {
        if let Some((namespace, name)) = named_type_key(avro_type) {
            let shape = *named.next().expect("survey saw every named type");
            let defs = given.entry((namespace.clone(), name.clone())).or_default();
            let name_given = match defs.iter().find(|(s, _)| *s == shape) {
                Some((_, g)) => g.clone(),
                None => {
                    let g = if defs.is_empty() {
                        name.clone()
                    } else {
                        let fresh = (1..)
                            .map(|i| format!("{name}_{i}"))
                            .find(|n| !taken.contains(&(namespace.clone(), n.clone())))
                            .unwrap();
                        taken.insert((namespace.clone(), fresh.clone()));
                        fresh
                    };
                    defs.push((shape, g.clone()));
                    g
                }
            };
            if name_given != name {
                avro_type["name"] = Value::String(name_given);
            }
        }
        match avro_type {
            Value::Object(obj) => obj
                .values_mut()
                .for_each(|v| rename(v, named, given, taken)),
            Value::Array(arr) => arr.iter_mut().for_each(|v| rename(v, named, given, taken)),
            _ => {}
        }
    }

    let mut taken = HashSet::new();
    let mut count = 0;
    for t in avro_schema.iter() {
        collect_names(t, &mut taken, &mut count);
    }
    if taken.len() == count {
        return; // every full name is defined once, so nothing can need renaming
    }
    let mut named = Vec::new();
    {
        let mut ids = ShapeIds::default();
        for t in avro_schema.iter() {
            survey(t, false, &mut ids, &mut named);
        }
    }
    let mut given = Given::new();
    let mut named = named.iter();
    for t in avro_schema.iter_mut() {
        rename(t, &mut named, &mut given, &mut taken);
    }
}

/// Remove the converter's bookkeeping keys from every type in an Avro schema.
///
/// `const` and `default` values are data, so they are left as they are.
pub fn strip_bookkeeping(avro_type: &mut Value) {
    match avro_type {
        Value::Object(obj) => {
            if obj.contains_key("type") && BOOKKEEPING_KEYS.iter().any(|k| obj.contains_key(*k)) {
                *obj = std::mem::take(obj)
                    .into_iter()
                    .filter(|(k, _)| !BOOKKEEPING_KEYS.contains(&k.as_str()))
                    .collect();
            }
            for (k, v) in obj.iter_mut() {
                if k != "const" && k != "default" {
                    strip_bookkeeping(v);
                }
            }
        }
        Value::Array(arr) => arr.iter_mut().for_each(strip_bookkeeping),
        _ => {}
    }
}
