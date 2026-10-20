//! `disambiguate_named_types` must name types exactly as 0.1.2 did, which compared
//! cloned, bookkeeping-stripped subtrees.
use avrotize::converter::postprocess::{disambiguate_named_types, strip_bookkeeping};
use serde_json::{json, Map, Value};
use std::collections::{HashMap, HashSet};

/// 0.1.2's `disambiguate_named_types`, kept as the reference.
fn disambiguate_0_1_2(avro_schema: &mut [Value]) {
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
    fn collect_names(avro_type: &Value, taken: &mut HashSet<(String, String)>) {
        if let Some(key) = named_type_key(avro_type) {
            taken.insert(key);
        }
        match avro_type {
            Value::Object(obj) => obj.values().for_each(|v| collect_names(v, taken)),
            Value::Array(arr) => arr.iter().for_each(|v| collect_names(v, taken)),
            _ => {}
        }
    }
    type Seen = HashMap<(String, String), Vec<(Value, String)>>;
    fn rename(avro_type: &mut Value, seen: &mut Seen, taken: &mut HashSet<(String, String)>) {
        if let Some((namespace, name)) = named_type_key(avro_type) {
            let mut shape = avro_type.clone();
            strip_bookkeeping(&mut shape);
            let defs = seen.entry((namespace.clone(), name.clone())).or_default();
            let given = match defs.iter().find(|(s, _)| *s == shape) {
                Some((_, given)) => given.clone(),
                None if defs.is_empty() => name.clone(),
                None => {
                    let fresh = (1..)
                        .map(|i| format!("{name}_{i}"))
                        .find(|n| !taken.contains(&(namespace.clone(), n.clone())))
                        .unwrap();
                    taken.insert((namespace.clone(), fresh.clone()));
                    fresh
                }
            };
            if !defs.iter().any(|(s, _)| *s == shape) {
                defs.push((shape, given.clone()));
            }
            if given != name {
                avro_type["name"] = Value::String(given);
            }
        }
        match avro_type {
            Value::Object(obj) => obj.values_mut().for_each(|v| rename(v, seen, taken)),
            Value::Array(arr) => arr.iter_mut().for_each(|v| rename(v, seen, taken)),
            _ => {}
        }
    }
    let mut taken = HashSet::new();
    avro_schema
        .iter()
        .for_each(|t| collect_names(t, &mut taken));
    let mut seen = Seen::new();
    avro_schema
        .iter_mut()
        .for_each(|t| rename(t, &mut seen, &mut taken));
}

struct Rng(u64);

impl Rng {
    fn below(&mut self, n: u64) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0 % n
    }
    fn pick<'a>(&mut self, xs: &[&'a str]) -> &'a str {
        xs[self.below(xs.len() as u64) as usize]
    }
}

/// An object from `entries`, inserted in a random order (equality ignores order).
fn shuffled(rng: &mut Rng, mut entries: Vec<(&str, Value)>) -> Value {
    let mut obj = Map::new();
    while !entries.is_empty() {
        let (k, v) = entries.remove(rng.below(entries.len() as u64) as usize);
        obj.insert(k.to_string(), v);
    }
    Value::Object(obj)
}

/// Data for `const` or `default`, sometimes shaped like a named type or carrying
/// bookkeeping-named keys.
fn data(rng: &mut Rng) -> Value {
    match rng.below(5) {
        0 => json!(rng.below(3)),
        1 => json!("u"),
        2 => json!({"dependencies": ["kept"], "x": 1}),
        3 => json!({"type": "record", "name": "r", "namespace": "ns", "fields": []}),
        _ => json!([1, {"type": "enum", "name": "e", "namespace": "ns", "symbols": ["a"]}]),
    }
}

/// `value` with its keys reshuffled and its bookkeeping keys changed: equal to it
/// for naming purposes, but not as a `Value`. Some echoes also change a
/// bookkeeping-named key inside `const` or `default` data, which makes them differ.
fn echo(rng: &mut Rng, value: &Value) -> Value {
    match value {
        Value::Object(obj) => {
            let mut entries: Vec<(&str, Value)> = obj
                .iter()
                .filter(|(k, _)| !matches!(k.as_str(), "dependencies" | "unmerged_types"))
                .map(|(k, v)| {
                    let data = k == "const" || k == "default";
                    let echoed = match v {
                        // Data keeps its keys, so this makes the echo a different shape.
                        Value::Object(d) if data && d.contains_key("type") && rng.below(2) == 0 => {
                            let mut d = d.clone();
                            d.insert("dependencies".into(), json!(["in data"]));
                            Value::Object(d)
                        }
                        _ if data => v.clone(),
                        _ => echo(rng, v),
                    };
                    (k.as_str(), echoed)
                })
                .collect();
            if obj.contains_key("type") && rng.below(2) == 0 {
                entries.push(("dependencies", json!(["other"])));
            }
            shuffled(rng, entries)
        }
        Value::Array(arr) => Value::Array(arr.iter().map(|v| echo(rng, v)).collect()),
        other => other.clone(),
    }
}

/// A random Avro type tree with a small pool of names, so full names collide often.
/// Named types go into `pool`, and some later types are echoes of earlier ones.
fn avro_type(rng: &mut Rng, depth: u32, pool: &mut Vec<Value>) -> Value {
    if !pool.is_empty() && rng.below(6) == 0 {
        let earlier = pool[rng.below(pool.len() as u64) as usize].clone();
        return echo(rng, &earlier);
    }
    let made = avro_type_new(rng, depth, pool);
    if made.get("name").is_some() {
        pool.push(made.clone());
    }
    made
}

fn avro_type_new(rng: &mut Rng, depth: u32, pool: &mut Vec<Value>) -> Value {
    const NAMES: [&str; 4] = ["r", "r_1", "s", "e"];
    const NAMESPACES: [&str; 2] = ["ns", "ns.sub"];
    const PRIMS: [&str; 4] = ["null", "int", "string", "boolean"];
    if depth == 0 || rng.below(4) == 0 {
        return json!(rng.pick(&PRIMS));
    }
    match rng.below(6) {
        0 | 1 => {
            let fields: Vec<Value> = (0..1 + rng.below(3))
                .map(|i| {
                    let mut field = vec![
                        ("name", json!(format!("f{}", rng.below(3) + i))),
                        ("type", avro_type(rng, depth - 1, pool)),
                    ];
                    if rng.below(5) == 0 {
                        field.push(("const", data(rng)));
                    }
                    if rng.below(5) == 0 {
                        field.push(("default", data(rng)));
                    }
                    shuffled(rng, field)
                })
                .collect();
            let mut record = vec![
                ("type", json!("record")),
                ("name", json!(rng.pick(&NAMES))),
                ("namespace", json!(rng.pick(&NAMESPACES))),
                ("fields", Value::Array(fields)),
            ];
            if rng.below(3) == 0 {
                record.push(("dependencies", json!([rng.pick(&NAMES)])));
            }
            if rng.below(8) == 0 {
                record.push(("unmerged_types", json!([])));
            }
            shuffled(rng, record)
        }
        2 => {
            let symbols: Vec<Value> = (0..1 + rng.below(2))
                .map(|_| json!(rng.pick(&["a", "b"])))
                .collect();
            let enum_type = vec![
                ("type", json!("enum")),
                ("name", json!(rng.pick(&NAMES))),
                ("namespace", json!(rng.pick(&NAMESPACES))),
                ("symbols", Value::Array(symbols)),
            ];
            shuffled(rng, enum_type)
        }
        3 => json!({"type": "array", "items": avro_type(rng, depth - 1, pool)}),
        4 => json!({"type": "map", "values": avro_type(rng, depth - 1, pool)}),
        _ => Value::Array(
            (0..2 + rng.below(2))
                .map(|_| avro_type(rng, depth - 1, pool))
                .collect(),
        ),
    }
}

#[test]
fn names_match_0_1_2_on_generated_schemas() {
    let mut rng = Rng(0x2545_F491_4F6C_DD1D);
    let mut renamed = 0;
    for _ in 0..3_000 {
        let items = 1 + rng.below(3);
        let mut pool = Vec::new();
        let schema: Vec<Value> = (0..items)
            .map(|_| avro_type(&mut rng, 5, &mut pool))
            .collect();
        let mut expected = schema.clone();
        disambiguate_0_1_2(&mut expected);
        let mut actual = schema.clone();
        disambiguate_named_types(&mut actual);
        // Compare the text too, so key order must match as well as content.
        assert_eq!(
            serde_json::to_string(&actual).unwrap(),
            serde_json::to_string(&expected).unwrap(),
            "input: {}",
            Value::Array(schema)
        );
        renamed += usize::from(actual != schema);
    }
    // The generator must actually produce collisions for this test to mean anything.
    assert!(renamed > 500, "only {renamed} schemas had a rename");
}
