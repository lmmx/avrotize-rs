#!/usr/bin/env bash
# Fuzz the CLI for unsound output: random JSON Schemas covering the #8 checklist.
#
#   tools/soundness/run.sh <n> <seed> [outdir]
#
# Builds jsonschema2avro and a strict Avro parser (apache-avro, the Rust crate, as its
# own crate under target/), then runs soundness.py, which checks every output with
# fastavro and the strict parser, and checks that instances jsonschema accepts fit the
# Avro schema. With outdir, one schema per failure message is saved there, and
# reduce.py shrinks them (run it with AVROCHECK set to the parser built here):
#
#   AVROCHECK=target/soundness-rig/avrocheck/target/release/avrocheck \
#     python3 tools/soundness/reduce.py target/release/jsonschema2avro <outdir>/examples.json
#
# Needs a Python ($PYTHON, default python3) with fastavro and jsonschema. Property
# names include x-y and 1st, which strict parsers reject as field names (the converter
# doesn't sanitize them); set SAFE_KEYS=1 to leave those out.
set -euo pipefail
repo=$(cd "$(dirname "$0")/../.." && pwd)
crate=$repo/target/soundness-rig/avrocheck
mkdir -p "$crate/src"
cat > "$crate/Cargo.toml" <<TOML
[package]
name = "avrocheck"
version = "0.0.0"
edition = "2021"
publish = false

[dependencies]
apache-avro = "0.21"
serde_json = "1"

[workspace]
TOML
cat > "$crate/src/main.rs" <<'RS'
// Parse each line of stdin (an Avro schema) with apache-avro; print OK or the error.
use std::io::BufRead;
fn main() {
    for line in std::io::stdin().lock().lines() {
        let v: serde_json::Value = serde_json::from_str(&line.unwrap()).unwrap();
        match apache_avro::Schema::parse(&v) {
            Ok(_) => println!("OK"),
            Err(e) => println!("ERR {e}"),
        }
    }
}
RS
(cd "$crate" && cargo build -q --release)
(cd "$repo" && cargo build -q --release -F cli)
cd "$(dirname "$0")"
AVROCHECK=$crate/target/release/avrocheck exec "${PYTHON:-python3}" soundness.py \
  "$repo/target/release/jsonschema2avro" "$@"
