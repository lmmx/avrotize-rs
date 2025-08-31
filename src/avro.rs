use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AvroType {
    Primitive(String),
    Record {
        #[serde(rename = "type")]
        r#type: String,
        name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        namespace: Option<String>,
        fields: Vec<AvroField>,
        #[serde(skip_serializing_if = "Option::is_none")]
        doc: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        dependencies: Option<Vec<String>>,
    },
    Enum {
        #[serde(rename = "type")]
        r#type: String,
        name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        namespace: Option<String>,
        symbols: Vec<String>,
    },
    Array {
        #[serde(rename = "type")]
        r#type: String,
        items: Box<AvroType>,
    },
    Map {
        #[serde(rename = "type")]
        r#type: String,
        values: Box<AvroType>,
    },
    Union(Vec<AvroType>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AvroField {
    pub name: String,
    #[serde(rename = "type")]
    pub field_type: AvroType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
}
