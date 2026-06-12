//! Plugin system error types.

use thiserror::Error;

/// Errors raised while discovering, loading, or calling plugins.
#[derive(Debug, Error)]
pub enum PluginError {
    #[error("plugin i/o error on {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("invalid plugin manifest at {path}: {message}")]
    Manifest { path: String, message: String },
    #[error("failed to load plugin from {path}: {message}")]
    Load { path: String, message: String },
    #[error("plugin ABI version mismatch: host expects {host}, plugin built against {plugin}")]
    AbiVersion { host: u32, plugin: u32 },
    #[error("wasm plugin error: {0}")]
    Wasm(String),
    #[error("plugin `{name}` not found in {dir}")]
    NotFound { name: String, dir: String },
    #[error("plugin `{name}` is `{actual}`, expected `{expected}`")]
    KindMismatch {
        name: String,
        expected: String,
        actual: String,
    },
    #[error("plugin call failed: {0}")]
    Call(String),
    #[error("{0}")]
    Other(String),
}

impl PluginError {
    pub(crate) fn io(path: &std::path::Path, source: std::io::Error) -> Self {
        PluginError::Io {
            path: path.display().to_string(),
            source,
        }
    }
}
