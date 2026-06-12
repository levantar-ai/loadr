//! Example WASM assertion plugin: fails when the response body exceeds a
//! configured size.
//!
//! Config: `{"max_body_bytes": 1024}`.

wit_bindgen::generate!({
    path: "../../../crates/loadr-plugin-api/wit",
    world: "loadr-assertion-plugin",
});

use exports::loadr::plugin::assertion::{Guest as Assertion, Verdict};
use exports::loadr::plugin::meta::{Guest as Meta, Info};

#[derive(serde::Deserialize)]
struct Config {
    max_body_bytes: u64,
}

struct Plugin;

impl Meta for Plugin {
    fn describe() -> Info {
        Info {
            name: "max-body-size".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            kind: "assertion".to_string(),
            description: "Fails when the response body exceeds max_body_bytes".to_string(),
        }
    }
}

impl Assertion for Plugin {
    fn check(
        _status: i64,
        body: Vec<u8>,
        _headers: Vec<(String, String)>,
        _duration_ms: f64,
        config: String,
    ) -> Verdict {
        let config: Config = match serde_json::from_str(&config) {
            Ok(c) => c,
            Err(e) => {
                return Verdict {
                    pass: false,
                    detail: format!("invalid config: {e}"),
                }
            }
        };
        let size = body.len() as u64;
        if size <= config.max_body_bytes {
            Verdict {
                pass: true,
                detail: format!("body is {size} bytes (limit {})", config.max_body_bytes),
            }
        } else {
            Verdict {
                pass: false,
                detail: format!(
                    "body is {size} bytes, exceeds limit of {} bytes",
                    config.max_body_bytes
                ),
            }
        }
    }
}

export!(Plugin);
