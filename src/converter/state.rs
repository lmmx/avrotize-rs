use crate::avro::AvroType;
use std::collections::HashMap;

/// Holds the state for converting JSON Schema → Avro Schema.
pub struct JsonToAvroConverter {
    pub imported_types: HashMap<String, AvroType>,
    pub root_namespace: String,
    pub max_recursion_depth: usize,
    pub content_cache: HashMap<String, String>,
    pub utility_namespace: String,
    pub split_top_level_records: bool,
    pub root_class_name: String,
}

impl Default for JsonToAvroConverter {
    fn default() -> Self {
        Self::new()
    }
}

impl JsonToAvroConverter {
    /// Create a new converter with default settings.
    pub fn new() -> Self {
        Self {
            imported_types: HashMap::new(),
            root_namespace: "example.com".to_string(),
            max_recursion_depth: 40,
            content_cache: HashMap::new(),
            utility_namespace: "utility.vasters.com".to_string(),
            split_top_level_records: false,
            root_class_name: "document".to_string(),
        }
    }
}
