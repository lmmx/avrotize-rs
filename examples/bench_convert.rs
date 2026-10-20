//! Time `jsons_to_avro` on generated schemas.
//!
//! ```sh
//! cargo run --release --example bench_convert -- <shape> <size> [runs]
//! ```
//!
//! Shapes:
//! - `wide`: `size` properties, each a `{language, value}` record (genson's label maps)
//! - `narrow`: one chain of `size` nested records
//! - `deep`: 40 side-by-side chains of `size` nested records
//! - `collide`: a key holding `size` differently shaped records, at three depths
//! - `genson`: `size` random genson-like schemas (seeded), converted one after another
//! - `file:<path>`: a JSON Schema read from disk (`size` is ignored)
//!
//! With `runs` of 0 it only generates the schemas, as a baseline for instruction or
//! allocation counts. Otherwise it prints the median and minimum wall time over
//! `runs` and a hash of the output,
//! which must not change between versions unless the output does. Set
//! `BENCH_NO_HASH=1` to skip the hash, e.g. when counting instructions or
//! allocations of the conversion alone. Only the public API is used, so the same
//! file builds against any avrotize version.
use serde_json::{json, Map, Value};
use std::hash::{Hash, Hasher};
use std::time::Instant;

fn wide(n: usize) -> Value {
    let label = json!({"type": "object", "required": ["language", "value"],
        "properties": {"language": {"type": "string"}, "value": {"type": "string"}}});
    let props: Map<String, Value> = (0..n).map(|i| (format!("k{i}"), label.clone())).collect();
    json!({"type": "object", "properties": props})
}

fn chain(depth: usize) -> Value {
    let mut props = Map::new();
    props.insert("a".into(), json!({"type": "integer"}));
    props.insert("b".into(), json!({"type": ["null", "string"]}));
    if depth > 0 {
        props.insert(format!("c{depth}"), chain(depth - 1));
    }
    json!({"type": "object", "required": ["a"], "properties": props})
}

fn deep(depth: usize) -> Value {
    let props: Map<String, Value> = (0..40).map(|i| (format!("k{i}"), chain(depth))).collect();
    json!({"type": "object", "properties": props})
}

fn collide(m: usize) -> Value {
    let shapes: Vec<Value> = (0..m)
        .map(|j| {
            json!({"type": "object", "required": [format!("f{j}")],
            "properties": {format!("f{j}"): {"type": "integer"}, "g": {"type": "string"}}})
        })
        .collect();
    let r = json!({"anyOf": shapes});
    json!({"type": "object", "properties": {
        "r": r,
        "x": {"type": "object", "properties": {"r": r}},
        "y": {"type": "array", "items": {"type": "object", "properties": {
            "z": {"type": "object", "properties": {"r": r}}}}}}})
}

/// A small xorshift generator, so the random schemas are the same everywhere.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

/// A random schema of the kind genson infers: objects, arrays, unions and primitives.
fn genson_like(rng: &mut Rng, depth: u32) -> Value {
    const KEYS: [&str; 6] = ["r", "d", "a", "x", "kind", "value"];
    const PRIMS: [&str; 5] = ["integer", "string", "boolean", "null", "number"];
    let roll = rng.below(100);
    if depth == 0 || roll < 35 {
        return match rng.below(20) {
            0..=2 => json!({"type": ["null", PRIMS[rng.below(3) as usize]]}),
            3 => json!({"const": if rng.below(2) == 0 { "u" } else { "v" }}),
            _ => json!({"type": PRIMS[rng.below(5) as usize]}),
        };
    }
    if roll < 65 {
        let mut props = Map::new();
        let mut required = Vec::new();
        for _ in 0..=rng.below(3) {
            let key = KEYS[rng.below(KEYS.len() as u64) as usize];
            props.insert(key.into(), genson_like(rng, depth - 1));
            if rng.below(2) == 0 {
                required.push(json!(key));
            }
        }
        return json!({"type": "object", "properties": props, "required": required});
    }
    if roll < 80 {
        return json!({"type": "array", "items": genson_like(rng, depth - 1)});
    }
    let branches: Vec<Value> = (0..2 + rng.below(2))
        .map(|_| genson_like(rng, depth - 1))
        .collect();
    json!({"anyOf": branches})
}

fn genson(n: usize) -> Vec<Value> {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    (0..n)
        .map(|_| {
            let depth = 2 + rng.below(4) as u32;
            json!({"type": "object", "required": ["r"], "properties": {"r": genson_like(&mut rng, depth)}})
        })
        .collect()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let usage = "usage: bench_convert <wide|narrow|deep|collide|genson|file:PATH> <size> [runs]";
    let shape = args.get(1).expect(usage);
    let size: usize = args.get(2).map_or(0, |s| s.parse().expect(usage));
    let runs: usize = args.get(3).map_or(5, |s| s.parse().expect(usage));

    let schemas = match shape.as_str() {
        "wide" => vec![wide(size)],
        "narrow" => vec![chain(size)],
        "deep" => vec![deep(size)],
        "collide" => vec![collide(size)],
        "genson" => genson(size),
        path if path.starts_with("file:") => {
            let text = std::fs::read_to_string(&path["file:".len()..]).expect("readable schema");
            vec![serde_json::from_str(&text).expect("JSON schema")]
        }
        _ => panic!("{usage}"),
    };

    let hash_output = std::env::var_os("BENCH_NO_HASH").is_none();
    let mut times = Vec::with_capacity(runs);
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for run in 0..runs {
        let start = Instant::now();
        let outputs: Vec<Value> = schemas
            .iter()
            .map(|s| avrotize::converter::jsons_to_avro(s, "bench", "", "", false))
            .collect();
        times.push(start.elapsed().as_secs_f64());
        if run == 0 && hash_output {
            outputs.iter().for_each(|o| o.to_string().hash(&mut hasher));
        }
    }
    if runs == 0 {
        // Generate only: a baseline to subtract when counting a conversion's own cost.
        println!("{shape} {size}: generated {} schemas", schemas.len());
        return;
    }
    times.sort_by(|a, b| a.total_cmp(b));
    println!(
        "{shape} {size}: median {:.5}s  min {:.5}s  runs {runs}  hash {:016x}",
        times[runs / 2],
        times[0],
        hasher.finish()
    );
}
