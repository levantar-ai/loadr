//! Example WASM extractor plugin: a boundary extractor that UPPERCASES the
//! match (so tests can tell it apart from the builtin boundary extractor).
//!
//! Config: `{"left": "<<", "right": ">>"}`.

wit_bindgen::generate!({
    path: "../../../crates/loadr-plugin-api/wit",
    world: "loadr-plugin",
});

use exports::loadr::plugin::extractor::Guest as Extractor;
use exports::loadr::plugin::meta::{Guest as Meta, Info};

#[derive(serde::Deserialize)]
struct Config {
    left: String,
    right: String,
}

struct Plugin;

impl Meta for Plugin {
    fn describe() -> Info {
        Info {
            name: "upper-boundary".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            kind: "extractor".to_string(),
            description: "Boundary extractor that uppercases the matched value".to_string(),
        }
    }
}

impl Extractor for Plugin {
    fn extract(body: Vec<u8>, _headers: Vec<(String, String)>, config: String) -> Option<String> {
        let config: Config = serde_json::from_str(&config).ok()?;
        if config.left.is_empty() || config.right.is_empty() {
            return None;
        }
        let text = String::from_utf8_lossy(&body);
        let start = text.find(&config.left)? + config.left.len();
        let end = text[start..].find(&config.right)? + start;
        Some(text[start..end].to_uppercase())
    }
}

export!(Plugin);
