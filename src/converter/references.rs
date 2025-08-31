use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::time::Duration;
use url::Url;
use reqwest::blocking::Client;

/// A simple cache for fetched schema content.
pub struct ContentCache {
    cache: HashMap<String, String>,
}

impl ContentCache {
    pub fn new() -> Self {
        Self { cache: HashMap::new() }
    }

    pub fn get(&self, url: &str) -> Option<&String> {
        self.cache.get(url)
    }

    pub fn insert(&mut self, url: &str, content: String) {
        self.cache.insert(url.to_string(), content);
    }
}

/// Fetch schema text from a URL or file path, with caching.
pub fn fetch_content(url: &str, cache: &mut ContentCache) -> Result<String, String> {
    if let Some(cached) = cache.get(url) {
        return Ok(cached.clone());
    }

    let parsed = Url::parse(url).map_err(|e| format!("Invalid URL: {e}"))?;

    let content = match parsed.scheme() {
        "http" | "https" => {
            let client = Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .map_err(|e| format!("Client build error: {e}"))?;
            let resp = client.get(url)
                .send()
                .map_err(|e| format!("HTTP request error: {e}"))?;
            resp.text().map_err(|e| format!("Error reading response: {e}"))?
        }
        "file" | "" => {
            let path = parsed.to_file_path()
                .map_err(|_| format!("Invalid file URL: {url}"))?;
            fs::read_to_string(&path)
                .map_err(|e| format!("File read error from {path:?}: {e}"))?
        }
        _ => return Err(format!("Unsupported scheme: {}", parsed.scheme())),
    };

    cache.insert(url, content.clone());
    Ok(content)
}

/// Resolve a `$ref` reference inside a JSON schema.
///
/// Supports:
/// - HTTP(s) and file URLs,
/// - JSON Pointer fragments (`#/definitions/...`).
/// Also tolerates sloppy `#definitions/...` by normalizing to `#/definitions/...`.
pub fn resolve_reference(
    json_type: &Value,
    base_uri: &str,
    json_doc: &Value,
    cache: &mut ContentCache,
) -> Result<(Value, Value), String> {
    let ref_str = json_type.get("$ref")
        .and_then(|v| v.as_str())
        .ok_or("Missing $ref")?;

    let parsed = Url::options()
        .base_url(Some(&Url::parse(base_uri).unwrap()))
        .parse(ref_str)
        .map_err(|e| format!("Invalid $ref {ref_str}: {e}"))?;

    let mut content = None;
    if ["http", "https", "file"].contains(&parsed.scheme()) {
        let text = fetch_content(parsed.as_str(), cache)?;
        content = Some(text);
    }

    let schema_doc: Value = if let Some(txt) = content {
        serde_json::from_str(&txt).map_err(|e| format!("JSON parse error: {e}"))?
    } else {
        json_doc.clone()
    };

    if let Some(fragment) = parsed.fragment() {
        // normalize fragment to RFC6901 JSON Pointer
        let pointer = if fragment.starts_with('/') {
            fragment.to_string()
        } else {
            format!("/{}", fragment)
        };

        let resolved = schema_doc.pointer(&pointer)
            .ok_or_else(|| format!("Invalid JSON Pointer fragment: {pointer}"))?;
        Ok((resolved.clone(), schema_doc))
    } else {
        Ok((schema_doc.clone(), schema_doc))
    }
}