//! Script engine abstraction. `loadr-js` implements these traits with QuickJS;
//! the engine only sees trait objects, keeping the runtime swappable.

use crate::error::ScriptError;

/// Factory for per-VU script instances plus run-level lifecycle hooks.
pub trait ScriptEngine: Send + Sync {
    /// Run `setup()` (if exported). The returned JSON is shared with every VU
    /// and passed to scenario functions as their first argument.
    fn setup(&self, host: &mut dyn ScriptHost) -> Result<serde_json::Value, ScriptError>;

    /// Run `teardown(setupData)` (if exported).
    fn teardown(
        &self,
        host: &mut dyn ScriptHost,
        setup_data: serde_json::Value,
    ) -> Result<(), ScriptError>;

    /// Create an isolated runtime for one VU.
    fn instantiate(&self) -> Result<Box<dyn VuScript>, ScriptError>;

    /// True when the module exports the named function.
    fn has_function(&self, name: &str) -> bool;
}

/// A per-VU script runtime. Calls are synchronous; the engine invokes them via
/// `block_in_place`, and host functions may block on async I/O internally.
pub trait VuScript: Send {
    /// Call an exported function: `fn(setup_data, context)`. Returns its result.
    fn call_function(
        &mut self,
        host: &mut dyn ScriptHost,
        name: &str,
        args: &[serde_json::Value],
    ) -> Result<serde_json::Value, ScriptError>;

    /// Evaluate an expression/snippet in the VU's context and return its value.
    fn eval(
        &mut self,
        host: &mut dyn ScriptHost,
        code: &str,
    ) -> Result<serde_json::Value, ScriptError>;

    fn has_function(&self, name: &str) -> bool;
}

/// Log levels for script `console.*` output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptLogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

/// An HTTP request issued from script code.
#[derive(Debug, Clone, Default)]
pub struct HostHttpRequest {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
    pub timeout_ms: Option<f64>,
    /// Extra tags for the request's samples.
    pub tags: Vec<(String, String)>,
    /// Metric name override.
    pub name: Option<String>,
}

/// The response handed back to script code.
#[derive(Debug, Clone, Default)]
pub struct HostHttpResponse {
    pub status: i64,
    pub status_text: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    pub duration_ms: f64,
    pub timings: crate::protocol::Timings,
    pub error: Option<String>,
    pub url: String,
    pub protocol_version: String,
}

/// Host services exposed to scripts. Implemented by the engine; methods are
/// synchronous from the script's perspective.
pub trait ScriptHost {
    /// Execute an HTTP request (blocking until the response arrives).
    fn http_request(&mut self, req: HostHttpRequest) -> HostHttpResponse;

    /// `sleep(seconds)`.
    fn sleep(&mut self, seconds: f64);

    /// Record a `checks` sample.
    fn check(&mut self, name: &str, pass: bool);

    /// Record to a custom metric (registering it on first use).
    fn metric_add(
        &mut self,
        metric: &str,
        kind: crate::metrics::MetricKind,
        value: f64,
        tags: &[(String, String)],
    ) -> Result<(), String>;

    /// Enter/leave a named group (affects sample tags).
    fn group_push(&mut self, name: &str);
    fn group_pop(&mut self);

    /// console.log and friends.
    fn log(&mut self, level: ScriptLogLevel, message: &str);

    /// `__ENV.NAME`.
    fn env_var(&self, name: &str) -> Option<String>;

    /// `open(path)` — read a data file relative to the test definition.
    fn open_file(&self, path: &str) -> Result<Vec<u8>, String>;

    /// Per-VU variable store shared with YAML `${...}` interpolation.
    fn get_var(&self, name: &str) -> Option<serde_json::Value>;
    fn set_var(&mut self, name: &str, value: serde_json::Value);

    /// Cookie jar access (manual cookie management).
    fn cookie_get(&self, url: &str, name: &str) -> Option<String>;
    fn cookie_set(&mut self, url: &str, name: &str, value: &str);
    fn cookies_clear(&mut self);

    /// Info about the executing VU: (vu_id, iteration, scenario).
    fn vu_info(&self) -> (u64, u64, String);

    /// Data row access: `data('source')` returns the current row as JSON.
    fn data_row(&mut self, source: &str) -> Result<serde_json::Value, String>;
}
