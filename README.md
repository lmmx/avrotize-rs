# avrotize

Rust port of [avrotize](https://github.com/oslabs-beta/avrotize), a tool to convert [JSON Schema](https://json-schema.org/) into [Apache Avro](https://avro.apache.org/) schemas.

## ✨ Overview

`avrotize-rs` is a high-performance converter that reads JSON Schema documents and produces equivalent Avro schema files.
It aims to be feature-complete with the original Python [avrotize](https://github.com/oslabs-beta/avrotize), while leveraging Rust’s speed, memory safety, and ecosystem.

Supported features include:

* ✅ Object → Avro `record`
* ✅ Arrays → Avro `array`
* ✅ `$defs` and `$ref` resolution
* ✅ Enums (`enum`) and constants (`const`)
* ✅ Required vs optional → nullable unions in Avro
* ✅ Descriptions → Avro `doc` fields
* ✅ Maps (`additionalProperties`)
* ✅ Composition (`oneOf`, `anyOf`, `allOf`)

- For unsupported features see [roadmap](https://github.com/lmmx/avrotize-rs/issues/8)

## 🚀 Usage

Convert a JSON Schema file to Avro:

```bash
cargo run -F cli
Usage: jsonschema2avro <JSONSCHEMA> <AVRO>
```

## 🧪 Tests

Fixtures live under `tests/fixtures/jsonschema/`.

For each fixture, an Avro schema is generated into `tests/fixtures/avro/` and compared against a snapshot in `tests/snapshots/`.

## 📋 Roadmap

* [ ] More robust external `$ref` resolution
* [ ] Additional Avro features (fixed, logical types)

## Acknowledgements

* Original [avrotize](https://github.com/clemensv/avrotize/) by Clemens Vasters.
* [Apache Avro](https://avro.apache.org/) project
* [difftastic](https://difftastic.wilfred.me.uk/) for beautiful test snapshot diffs
