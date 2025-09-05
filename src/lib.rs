#![cfg_attr(feature = "trace", allow(clippy::too_many_arguments))] // crustrace::omni expands fn signatures
//! # avrotize
//!
//! Convert [JSON Schema](https://json-schema.org/) documents into
//! [Apache Avro](https://avro.apache.org/) schemas.
//!
//! ## Features
//!
//! - Supports `$defs` / `definitions` resolution
//! - Handles composition keywords (`allOf`, `anyOf`, `oneOf`)
//! - Maps primitive JSON Schema types to Avro equivalents
//! - Generates records, enums, arrays, maps, and unions
//! - Resolves and sorts type dependencies
//! - CLI tool `jsonschema2avro` for batch conversion
//!
//! ## Example (Programmatic Usage)
//!
//! ```no_run
//! use serde_json::json;
//! use avrotize::converter::jsons_to_avro;
//!
//! let schema = json!({
//!     "$schema": "https://json-schema.org/draft/2020-12/schema",
//!     "title": "Example",
//!     "type": "object",
//!     "properties": {
//!         "name": { "type": "string" },
//!         "age": { "type": "integer" }
//!     },
//!     "required": ["name"]
//! });
//!
//! let avro = jsons_to_avro(
//!     &schema,
//!     "example_ns",          // namespace
//!     "example_ns.utility",  // utility namespace
//!     "example.json",        // base URI
//!     false                  // don't split top-level
//! );
//!
//! println!("{}", serde_json::to_string_pretty(&avro).unwrap());
//! ```
//!
//! ## Example (CLI)
//!
//! ```bash
//! jsonschema2avro schema.json out.avsc
//! ```
//!
//! Or to split top-level records into separate `.avsc` files:
//!
//! ```bash
//! jsonschema2avro schema.json out_dir --split-top-level-records
//! ```
//!
//! ## Crate Layout
//!
//! - [`avro`] — Core Avro type definitions (`AvroType`, `AvroField`)
//! - [`common`] — Helpers for names, hashing, traversal, etc.
//! - [`converter`] — JSON Schema → Avro conversion logic
//! - [`dependency_resolver`] — Handles dependency ordering and inlining
//!
//! The CLI binary is enabled with the `cli` feature.
pub mod avro;
pub mod common;
pub mod converter;
pub mod dependency_resolver;
