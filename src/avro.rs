use serde::{Deserialize, Serialize};

/// Represents the different forms of Avro types that can be generated
/// from JSON Schema input.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AvroType {
    /// A primitive Avro type, e.g. `"string"`, `"int"`, `"boolean"`.
    Primitive(String),
    /// An Avro record type with named fields.
    Record {
        #[serde(rename = "type")]
        r#type: String,
        name: String,
        /// Optional namespace for the record.
        #[serde(skip_serializing_if = "Option::is_none")]
        namespace: Option<String>,
        /// List of record fields.
        fields: Vec<AvroField>,
        /// Optional documentation string.
        #[serde(skip_serializing_if = "Option::is_none")]
        doc: Option<String>,
        /// Dependencies on other Avro types.
        #[serde(skip_serializing_if = "Option::is_none")]
        dependencies: Option<Vec<String>>,
    },
    /// An Avro enum type with symbols.
    Enum {
        #[serde(rename = "type")]
        r#type: String,
        /// Enum name.
        name: String,
        /// Optional namespace for the enum.
        #[serde(skip_serializing_if = "Option::is_none")]
        namespace: Option<String>,
        /// Allowed symbols in the enum.
        symbols: Vec<String>,
    },
    /// An Avro array type.
    Array {
        #[serde(rename = "type")]
        r#type: String,
        /// Item type contained in the array.
        items: Box<AvroType>,
    },
    /// An Avro map type with string keys.
    Map {
        #[serde(rename = "type")]
        r#type: String,
        /// Value type for map entries.
        values: Box<AvroType>,
    },
    /// A union of multiple Avro types.
    Union(Vec<AvroType>),
}

/// A field inside an Avro record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AvroField {
    /// Name of the field.
    pub name: String,
    /// Avro type of the field.
    #[serde(rename = "type")]
    pub field_type: AvroType,
    /// Optional documentation string.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
}
