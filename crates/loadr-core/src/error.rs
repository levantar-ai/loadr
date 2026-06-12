//! Engine error types.

use thiserror::Error;

/// Fatal engine errors (a failing request is NOT an error — it's a sample).
#[derive(Debug, Error)]
pub enum EngineError {
    #[error("configuration error: {0}")]
    Config(String),
    #[error("data source `{source_name}`: {message}")]
    Data {
        source_name: String,
        message: String,
    },
    #[error("script error: {0}")]
    Script(String),
    #[error("protocol `{protocol}` is not registered{hint}")]
    UnknownProtocol { protocol: String, hint: String },
    #[error("template error: {0}")]
    Template(#[from] loadr_config::TemplateError),
    #[error("i/o error on {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("run aborted: {0}")]
    Aborted(String),
    #[error("{0}")]
    Other(String),
}

/// Errors from a protocol handler executing one request.
/// These mark the request failed; they don't stop the run.
#[derive(Debug, Error)]
pub enum ProtocolError {
    #[error("connection failed: {0}")]
    Connect(String),
    #[error("tls error: {0}")]
    Tls(String),
    #[error("dns error: {0}")]
    Dns(String),
    #[error("request timed out after {0:?}")]
    Timeout(std::time::Duration),
    #[error("invalid request: {0}")]
    InvalidRequest(String),
    #[error("transport error: {0}")]
    Transport(String),
}

/// Errors from the embedded script runtime.
#[derive(Debug, Error)]
pub enum ScriptError {
    #[error("javascript exception: {0}")]
    Exception(String),
    #[error("script timed out (limit {0:?})")]
    Timeout(std::time::Duration),
    #[error("script exceeded memory limit")]
    OutOfMemory,
    #[error("function `{0}` is not exported by the test script")]
    NoSuchFunction(String),
    #[error("script runtime error: {0}")]
    Runtime(String),
}
