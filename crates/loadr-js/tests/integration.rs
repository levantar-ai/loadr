//! End-to-end tests for the QuickJS script engine, driven through a MockHost.

use std::collections::HashMap;
use std::path::Path;

use loadr_core::error::ScriptError;
use loadr_core::script::{
    HostHttpRequest, HostHttpResponse, ScriptEngine, ScriptHost, ScriptLogLevel,
};
use loadr_js::JsEngine;
use serde_json::json;

// ---------------------------------------------------------------------------
// MockHost
// ---------------------------------------------------------------------------

/// (metric, kind-as-str, value, tags)
type MetricSample = (String, String, f64, Vec<(String, String)>);

#[derive(Default)]
struct MockHost {
    requests: Vec<HostHttpRequest>,
    response_status: i64,
    response_body: String,
    response_headers: Vec<(String, String)>,
    sleeps: Vec<f64>,
    checks: Vec<(String, bool)>,
    /// (metric, kind-as-str, value, tags)
    metrics: Vec<MetricSample>,
    /// "push:<name>" / "pop" event log.
    group_events: Vec<String>,
    logs: Vec<(String, String)>,
    env: HashMap<String, String>,
    files: HashMap<String, Vec<u8>>,
    vars: HashMap<String, serde_json::Value>,
    cookies: HashMap<(String, String), String>,
    data_rows: HashMap<String, serde_json::Value>,
    vu: u64,
    iteration: u64,
    scenario: String,
}

impl MockHost {
    fn new() -> Self {
        MockHost {
            response_status: 200,
            response_body: r#"{"ok":true}"#.to_string(),
            response_headers: vec![("Content-Type".to_string(), "application/json".to_string())],
            vu: 7,
            iteration: 3,
            scenario: "main".to_string(),
            ..Default::default()
        }
    }
}

impl ScriptHost for MockHost {
    fn http_request(&mut self, req: HostHttpRequest) -> HostHttpResponse {
        let url = req.url.clone();
        self.requests.push(req);
        HostHttpResponse {
            status: self.response_status,
            status_text: "OK".to_string(),
            headers: self.response_headers.clone(),
            body: self.response_body.clone().into_bytes(),
            duration_ms: 12.5,
            timings: Default::default(),
            error: None,
            url,
            protocol_version: "HTTP/1.1".to_string(),
        }
    }

    fn sleep(&mut self, seconds: f64) {
        self.sleeps.push(seconds);
    }

    fn check(&mut self, name: &str, pass: bool) {
        self.checks.push((name.to_string(), pass));
    }

    fn metric_add(
        &mut self,
        metric: &str,
        kind: loadr_core::metrics::MetricKind,
        value: f64,
        tags: &[(String, String)],
    ) -> Result<(), String> {
        self.metrics.push((
            metric.to_string(),
            kind.as_str().to_string(),
            value,
            tags.to_vec(),
        ));
        Ok(())
    }

    fn group_push(&mut self, name: &str) {
        self.group_events.push(format!("push:{name}"));
    }

    fn group_pop(&mut self) {
        self.group_events.push("pop".to_string());
    }

    fn log(&mut self, level: ScriptLogLevel, message: &str) {
        let level = match level {
            ScriptLogLevel::Debug => "debug",
            ScriptLogLevel::Info => "info",
            ScriptLogLevel::Warn => "warn",
            ScriptLogLevel::Error => "error",
        };
        self.logs.push((level.to_string(), message.to_string()));
    }

    fn env_var(&self, name: &str) -> Option<String> {
        self.env.get(name).cloned()
    }

    fn open_file(&self, path: &str) -> Result<Vec<u8>, String> {
        self.files
            .get(path)
            .cloned()
            .ok_or_else(|| format!("file not found: {path}"))
    }

    fn get_var(&self, name: &str) -> Option<serde_json::Value> {
        self.vars.get(name).cloned()
    }

    fn set_var(&mut self, name: &str, value: serde_json::Value) {
        self.vars.insert(name.to_string(), value);
    }

    fn cookie_get(&self, url: &str, name: &str) -> Option<String> {
        self.cookies
            .get(&(url.to_string(), name.to_string()))
            .cloned()
    }

    fn cookie_set(&mut self, url: &str, name: &str, value: &str) {
        self.cookies
            .insert((url.to_string(), name.to_string()), value.to_string());
    }

    fn cookies_clear(&mut self) {
        self.cookies.clear();
    }

    fn vu_info(&self) -> (u64, u64, String) {
        (self.vu, self.iteration, self.scenario.clone())
    }

    fn data_row(&mut self, source: &str) -> Result<serde_json::Value, String> {
        self.data_rows
            .get(source)
            .cloned()
            .ok_or_else(|| format!("unknown data source: {source}"))
    }
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn engine(script: &str) -> JsEngine {
    let config = loadr_config::JsConfig {
        file: None,
        script: Some(script.to_string()),
        timeout: None,
        memory_limit_mb: None,
    };
    JsEngine::new(&config, Path::new(".")).expect("engine builds")
}

fn run_default(script: &str, host: &mut MockHost) -> serde_json::Value {
    let engine = engine(script);
    let mut vu = engine.instantiate().expect("instantiate");
    vu.call_function(host, "default", &[])
        .expect("default runs")
}

// ---------------------------------------------------------------------------
// tests
// ---------------------------------------------------------------------------

#[test]
fn detects_module_exports() {
    let engine = engine(
        r#"
export function setup() { return 1; }
export function helper() {}
export const notAFunction = 42;
export default function () {}
"#,
    );
    assert!(engine.has_function("setup"));
    assert!(engine.has_function("helper"));
    assert!(engine.has_function("default"));
    assert!(!engine.has_function("notAFunction"));
    assert!(!engine.has_function("teardown"));

    let vu = engine.instantiate().expect("instantiate");
    assert!(vu.has_function("default"));
    assert!(!vu.has_function("missing"));
}

#[test]
fn calling_missing_function_errors() {
    let engine = engine("export default function () {}");
    let mut vu = engine.instantiate().expect("instantiate");
    let mut host = MockHost::new();
    let err = vu
        .call_function(&mut host, "nope", &[])
        .expect_err("missing function");
    assert!(matches!(err, ScriptError::NoSuchFunction(name) if name == "nope"));
}

#[test]
fn default_receives_setup_data_and_context() {
    let engine = engine(
        r#"
export default function (data, ctx) {
  return { token: data.token, vu: ctx.vu, sum: data.n + 1 };
}
"#,
    );
    let mut vu = engine.instantiate().expect("instantiate");
    let mut host = MockHost::new();
    let result = vu
        .call_function(
            &mut host,
            "default",
            &[json!({"token": "abc", "n": 41}), json!({"vu": 7})],
        )
        .expect("call");
    assert_eq!(result, json!({"token": "abc", "vu": 7, "sum": 42}));
}

#[test]
fn http_get_round_trip() {
    let mut host = MockHost::new();
    let result = run_default(
        r#"
export default function () {
  const r = http.get('https://api.test/users', {
    headers: { 'X-Token': 'abc' },
    tags: { endpoint: 'users' },
    name: 'users',
    timeout: '30s',
  });
  return {
    status: r.status,
    ok: r.json().ok,
    ct: r.headers['content-type'],
    body: r.body,
    proto: r.protocol,
    err: r.error,
    dur: r.duration_ms,
  };
}
"#,
        &mut host,
    );
    assert_eq!(result["status"], json!(200));
    assert_eq!(result["ok"], json!(true));
    assert_eq!(result["ct"], json!("application/json"));
    assert_eq!(result["body"], json!(r#"{"ok":true}"#));
    assert_eq!(result["proto"], json!("HTTP/1.1"));
    assert_eq!(result["err"], json!(null));
    assert_eq!(result["dur"], json!(12.5));

    assert_eq!(host.requests.len(), 1);
    let req = &host.requests[0];
    assert_eq!(req.method, "GET");
    assert_eq!(req.url, "https://api.test/users");
    assert!(req
        .headers
        .iter()
        .any(|(k, v)| k == "X-Token" && v == "abc"));
    assert!(req
        .tags
        .iter()
        .any(|(k, v)| k == "endpoint" && v == "users"));
    assert_eq!(req.name.as_deref(), Some("users"));
    assert_eq!(req.timeout_ms, Some(30_000.0));
    assert!(req.body.is_none());
}

#[test]
fn http_post_json_body_sets_content_type() {
    let mut host = MockHost::new();
    run_default(
        r#"
export default function () {
  http.post('https://api.test/items', { a: 1 });
  http.post('https://api.test/raw', 'plain text', { headers: { 'Content-Type': 'text/plain' } });
}
"#,
        &mut host,
    );
    assert_eq!(host.requests.len(), 2);
    let first = &host.requests[0];
    assert_eq!(first.method, "POST");
    assert_eq!(first.body.as_deref(), Some(br#"{"a":1}"#.as_slice()));
    assert!(first
        .headers
        .iter()
        .any(|(k, v)| k == "Content-Type" && v == "application/json"));
    let second = &host.requests[1];
    assert_eq!(second.body.as_deref(), Some(b"plain text".as_slice()));
    assert!(second
        .headers
        .iter()
        .any(|(k, v)| k == "Content-Type" && v == "text/plain"));
}

#[test]
fn check_records_function_and_bool_styles() {
    let mut host = MockHost::new();
    let result = run_default(
        r#"
export default function () {
  const all = check({ status: 200 }, {
    'status ok': (r) => r.status === 200,
    'flag set': false,
    'literal true': true,
    'throws counts as fail': () => { throw new Error('x'); },
  });
  return all;
}
"#,
        &mut host,
    );
    assert_eq!(result, json!(false));
    let mut checks = host.checks.clone();
    checks.sort();
    assert_eq!(
        checks,
        vec![
            ("flag set".to_string(), false),
            ("literal true".to_string(), true),
            ("status ok".to_string(), true),
            ("throws counts as fail".to_string(), false),
        ]
    );
}

#[test]
fn check_returns_true_when_all_pass() {
    let mut host = MockHost::new();
    let result = run_default(
        "export default function () { return check(5, { five: (v) => v === 5 }); }",
        &mut host,
    );
    assert_eq!(result, json!(true));
}

#[test]
fn sleep_is_recorded() {
    let mut host = MockHost::new();
    run_default("export default function () { sleep(1.5); }", &mut host);
    assert_eq!(host.sleeps, vec![1.5]);
}

#[test]
fn group_push_pop_ordering_with_exceptions() {
    let mut host = MockHost::new();
    run_default(
        r#"
export default function () {
  group('outer', function () {
    try {
      group('inner', function () { throw new Error('boom'); });
    } catch (e) {
      // swallowed; the inner group must still have been popped
    }
  });
}
"#,
        &mut host,
    );
    assert_eq!(
        host.group_events,
        vec!["push:outer", "push:inner", "pop", "pop"]
    );
}

#[test]
fn group_returns_callback_result() {
    let mut host = MockHost::new();
    let result = run_default(
        "export default function () { return group('g', () => 42); }",
        &mut host,
    );
    assert_eq!(result, json!(42));
}

#[test]
fn custom_metrics_with_tags() {
    let mut host = MockHost::new();
    run_default(
        r#"
export default function () {
  const c = Counter('my_counter');
  c.add(2, { region: 'eu' });
  new Trend('latency').add(150.5, { phase: 'ramp' });
  Rate('errors').add(true);
  Gauge('depth').add(42);
}
"#,
        &mut host,
    );
    assert_eq!(
        host.metrics,
        vec![
            (
                "my_counter".to_string(),
                "counter".to_string(),
                2.0,
                vec![("region".to_string(), "eu".to_string())]
            ),
            (
                "latency".to_string(),
                "trend".to_string(),
                150.5,
                vec![("phase".to_string(), "ramp".to_string())]
            ),
            ("errors".to_string(), "rate".to_string(), 1.0, vec![]),
            ("depth".to_string(), "gauge".to_string(), 42.0, vec![]),
        ]
    );
}

#[test]
fn session_metric_conveniences() {
    let mut host = MockHost::new();
    run_default(
        r#"
export default function () {
  session.counterAdd('c', 1, { a: 'b' });
  session.gaugeSet('g', 9);
  session.rateAdd('r', false);
  session.trendAdd('t', 3.5);
}
"#,
        &mut host,
    );
    let kinds: Vec<(&str, &str, f64)> = host
        .metrics
        .iter()
        .map(|(n, k, v, _)| (n.as_str(), k.as_str(), *v))
        .collect();
    assert_eq!(
        kinds,
        vec![
            ("c", "counter", 1.0),
            ("g", "gauge", 9.0),
            ("r", "rate", 0.0),
            ("t", "trend", 3.5),
        ]
    );
}

#[test]
fn crypto_digests_match_known_vectors() {
    let mut host = MockHost::new();
    let result = run_default(
        r#"
export default function () {
  return {
    sha256: crypto.sha256('abc', 'hex'),
    sha1: crypto.sha1('abc', 'hex'),
    md5: crypto.md5('abc'),
    hmac: crypto.hmac('sha256', new Array(20).fill(0x0b), 'Hi There', 'hex'),
    b64digest: crypto.md5('abc', 'base64'),
    rand: crypto.randomBytes(16),
    uuid: crypto.uuidv4(),
  };
}
"#,
        &mut host,
    );
    assert_eq!(
        result["sha256"],
        json!("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
    );
    assert_eq!(
        result["sha1"],
        json!("a9993e364706816aba3e25717850c26c9cd0d89d")
    );
    assert_eq!(result["md5"], json!("900150983cd24fb0d6963f7d28e17f72"));
    // RFC 4231 test case 1
    assert_eq!(
        result["hmac"],
        json!("b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7")
    );
    assert_eq!(result["b64digest"], json!("kAFQmDzST7DWlj99KOF/cg=="));
    let rand = result["rand"].as_array().expect("array of bytes");
    assert_eq!(rand.len(), 16);
    let uuid = result["uuid"].as_str().expect("uuid string");
    assert_eq!(uuid.len(), 36);
}

#[test]
fn base64_round_trip() {
    let mut host = MockHost::new();
    let result = run_default(
        r#"
export default function () {
  const encoded = encoding.b64encode('hello world');
  return { encoded, decoded: encoding.b64decode(encoded) };
}
"#,
        &mut host,
    );
    assert_eq!(result["encoded"], json!("aGVsbG8gd29ybGQ="));
    assert_eq!(result["decoded"], json!("hello world"));
}

#[test]
fn env_reads_mock_environment() {
    let mut host = MockHost::new();
    host.env.insert("FOO".to_string(), "bar".to_string());
    let result = run_default(
        r#"
export default function () {
  return { foo: __ENV.FOO, missing: typeof __ENV.NOPE };
}
"#,
        &mut host,
    );
    assert_eq!(result["foo"], json!("bar"));
    assert_eq!(result["missing"], json!("undefined"));
}

#[test]
fn open_reads_text_and_binary_files() {
    let mut host = MockHost::new();
    host.files.insert("data.txt".to_string(), b"hello".to_vec());
    host.files
        .insert("blob.bin".to_string(), vec![0, 1, 2, 255]);
    let result = run_default(
        r#"
export default function () {
  const text = open('data.txt');
  const bin = open('blob.bin', 'b');
  return { text, len: bin.length, first: bin[0], last: bin[3] };
}
"#,
        &mut host,
    );
    assert_eq!(result["text"], json!("hello"));
    assert_eq!(result["len"], json!(4));
    assert_eq!(result["first"], json!(0));
    assert_eq!(result["last"], json!(255));
}

#[test]
fn open_missing_file_throws() {
    let engine = engine("export default function () { open('nope.txt'); }");
    let mut vu = engine.instantiate().expect("instantiate");
    let mut host = MockHost::new();
    let err = vu
        .call_function(&mut host, "default", &[])
        .expect_err("missing file");
    assert!(err.to_string().contains("nope.txt"), "got: {err}");
}

#[test]
fn console_logs_through_host() {
    let mut host = MockHost::new();
    run_default(
        r#"
export default function () {
  console.log('hello', { a: 1 });
  console.warn('careful');
  console.error('bad');
  console.debug('details');
}
"#,
        &mut host,
    );
    assert_eq!(
        host.logs,
        vec![
            ("info".to_string(), "hello {\"a\":1}".to_string()),
            ("warn".to_string(), "careful".to_string()),
            ("error".to_string(), "bad".to_string()),
            ("debug".to_string(), "details".to_string()),
        ]
    );
}

#[test]
fn session_vars_persist_across_calls_and_reach_host() {
    let engine = engine(
        r#"
export default function () {
  session.vars.count = (session.vars.count || 0) + 1;
  return session.vars.count;
}
"#,
    );
    let mut vu = engine.instantiate().expect("instantiate");
    let mut host = MockHost::new();
    let first = vu.call_function(&mut host, "default", &[]).expect("call 1");
    let second = vu.call_function(&mut host, "default", &[]).expect("call 2");
    assert_eq!(first, json!(1));
    assert_eq!(second, json!(2));
    assert_eq!(host.vars.get("count"), Some(&json!(2)));
}

#[test]
fn session_info_cookies_and_data() {
    let mut host = MockHost::new();
    host.data_rows
        .insert("users".to_string(), json!({"name": "ada"}));
    let result = run_default(
        r#"
export default function () {
  session.cookieSet('https://x.test', 'sid', 'abc');
  const row = session.data('users');
  const out = {
    vu: session.vu,
    iteration: session.iteration,
    scenario: session.scenario,
    sid: session.cookieGet('https://x.test', 'sid'),
    name: row.name,
  };
  session.cookiesClear();
  out.cleared = session.cookieGet('https://x.test', 'sid') === undefined ? true : false;
  return out;
}
"#,
        &mut host,
    );
    assert_eq!(result["vu"], json!(7));
    assert_eq!(result["iteration"], json!(3));
    assert_eq!(result["scenario"], json!("main"));
    assert_eq!(result["sid"], json!("abc"));
    assert_eq!(result["name"], json!("ada"));
    assert_eq!(result["cleared"], json!(true));
    assert!(host.cookies.is_empty());
}

#[test]
fn k6_style_imports_work() {
    let engine = engine(
        r#"
import http from 'k6/http';
import { check, sleep } from 'k6';

export default function () {
  const r = http.get('https://x/');
  check(r, { 'ok': (r2) => r2.status === 200 });
  sleep(0.1);
}
"#,
    );
    let mut vu = engine.instantiate().expect("instantiate");
    let mut host = MockHost::new();
    vu.call_function(&mut host, "default", &[]).expect("runs");
    assert_eq!(host.requests.len(), 1);
    assert_eq!(host.requests[0].url, "https://x/");
    assert_eq!(host.checks, vec![("ok".to_string(), true)]);
    assert_eq!(host.sleeps, vec![0.1]);
}

#[test]
fn k6_metrics_and_crypto_imports_work() {
    let mut host = MockHost::new();
    let engine = engine(
        r#"
import { Counter } from 'k6/metrics';
import { sha256 } from 'k6/crypto';
import encoding from 'k6/encoding';

const hits = new Counter('hits');

export default function () {
  hits.add(1);
  return { digest: sha256('abc', 'hex'), enc: encoding.b64encode('hi') };
}
"#,
    );
    let mut vu = engine.instantiate().expect("instantiate");
    let result = vu.call_function(&mut host, "default", &[]).expect("runs");
    assert_eq!(host.metrics[0].0, "hits");
    assert_eq!(
        result["digest"],
        json!("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
    );
    assert_eq!(result["enc"], json!("aGk="));
}

#[test]
fn unknown_import_is_a_clean_error() {
    let config = loadr_config::JsConfig {
        file: None,
        script: Some("import x from 'left-pad';\nexport default function () {}".to_string()),
        timeout: None,
        memory_limit_mb: None,
    };
    let err = JsEngine::new(&config, Path::new(".")).expect_err("unknown module");
    let text = err.to_string();
    assert!(text.contains("left-pad"), "names the module: {text}");
}

#[test]
fn eval_expression_and_statements() {
    let engine = engine("export default function () {}");
    let mut vu = engine.instantiate().expect("instantiate");
    let mut host = MockHost::new();

    assert_eq!(vu.eval(&mut host, "1+1").expect("expr"), json!(2));
    assert_eq!(vu.eval(&mut host, "'a' + 'b'").expect("expr"), json!("ab"));

    // Multi-statement snippets run for their side effects and return null.
    let result = vu
        .eval(&mut host, "session.vars.flag = 'yes'; const z = 1;")
        .expect("statements");
    assert_eq!(result, json!(null));
    assert_eq!(host.vars.get("flag"), Some(&json!("yes")));
}

#[test]
fn eval_sees_response_binding() {
    let engine = engine("export default function () {}");
    let mut vu = engine.instantiate().expect("instantiate");
    let mut host = MockHost::new();
    host.vars.insert(
        "response".to_string(),
        json!({"status": 201, "body": "made"}),
    );
    assert_eq!(
        vu.eval(&mut host, "response.status").expect("eval"),
        json!(201)
    );
    assert_eq!(
        vu.eval(
            &mut host,
            "response.status === 201 && response.body === 'made'"
        )
        .expect("eval"),
        json!(true)
    );
}

#[test]
fn eval_runtime_errors_are_exceptions() {
    let engine = engine("export default function () {}");
    let mut vu = engine.instantiate().expect("instantiate");
    let mut host = MockHost::new();
    let err = vu
        .eval(&mut host, "undefinedVariable.someField")
        .expect_err("reference error");
    assert!(matches!(err, ScriptError::Exception(_)), "got: {err:?}");
}

#[test]
fn infinite_loop_times_out() {
    let config = loadr_config::JsConfig {
        file: None,
        script: Some("export default function () { while (true) {} }".to_string()),
        timeout: Some(loadr_config::Dur::from_millis(200)),
        memory_limit_mb: None,
    };
    let engine = JsEngine::new(&config, Path::new(".")).expect("engine");
    let mut vu = engine.instantiate().expect("instantiate");
    let mut host = MockHost::new();
    let start = std::time::Instant::now();
    let err = vu
        .call_function(&mut host, "default", &[])
        .expect_err("must time out");
    assert!(matches!(err, ScriptError::Timeout(_)), "got: {err:?}");
    assert!(
        start.elapsed() < std::time::Duration::from_secs(5),
        "interrupt fired promptly"
    );
}

#[test]
fn runaway_allocation_errors() {
    let config = loadr_config::JsConfig {
        file: None,
        script: Some(
            "export default function () { let s = 'x'; while (true) { s += s; } }".to_string(),
        ),
        timeout: Some(loadr_config::Dur::from_secs(10)),
        memory_limit_mb: Some(8),
    };
    let engine = JsEngine::new(&config, Path::new(".")).expect("engine");
    let mut vu = engine.instantiate().expect("instantiate");
    let mut host = MockHost::new();
    let err = vu
        .call_function(&mut host, "default", &[])
        .expect_err("must exhaust memory");
    assert!(
        matches!(
            err,
            ScriptError::OutOfMemory | ScriptError::Exception(_) | ScriptError::Runtime(_)
        ),
        "got: {err:?}"
    );
}

#[test]
fn vus_are_isolated() {
    let engine = engine("export default function () {}");
    let mut vu1 = engine.instantiate().expect("vu1");
    let mut vu2 = engine.instantiate().expect("vu2");
    let mut host = MockHost::new();
    vu1.eval(&mut host, "globalThis.leak = 'secret'; 1")
        .expect("set global in vu1");
    assert_eq!(
        vu1.eval(&mut host, "globalThis.leak").expect("vu1 sees it"),
        json!("secret")
    );
    assert_eq!(
        vu2.eval(&mut host, "typeof globalThis.leak")
            .expect("vu2 does not"),
        json!("undefined")
    );
}

#[test]
fn promise_returning_default_is_resolved() {
    let mut host = MockHost::new();
    let result = run_default(
        r#"
export default async function () {
  const base = await Promise.resolve(40);
  return Promise.resolve(base).then((x) => x + 2);
}
"#,
        &mut host,
    );
    assert_eq!(result, json!(42));
}

#[test]
fn rejected_promise_is_an_exception() {
    let engine = engine("export default async function () { throw new Error('async boom'); }");
    let mut vu = engine.instantiate().expect("instantiate");
    let mut host = MockHost::new();
    let err = vu
        .call_function(&mut host, "default", &[])
        .expect_err("rejection");
    assert!(
        matches!(&err, ScriptError::Exception(msg) if msg.contains("async boom")),
        "got: {err:?}"
    );
}

#[test]
fn never_resolving_promise_is_a_clean_error() {
    let engine = engine("export default function () { return new Promise(() => {}); }");
    let mut vu = engine.instantiate().expect("instantiate");
    let mut host = MockHost::new();
    let err = vu
        .call_function(&mut host, "default", &[])
        .expect_err("pending forever");
    assert!(
        matches!(&err, ScriptError::Runtime(msg) if msg.contains("pending")),
        "got: {err:?}"
    );
}

#[test]
fn thrown_script_errors_carry_message_and_stack() {
    let engine = engine(
        r#"
export default function () {
  function inner() { throw new TypeError('bad thing'); }
  inner();
}
"#,
    );
    let mut vu = engine.instantiate().expect("instantiate");
    let mut host = MockHost::new();
    let err = vu
        .call_function(&mut host, "default", &[])
        .expect_err("throws");
    match err {
        ScriptError::Exception(text) => {
            assert!(text.contains("TypeError"), "has name: {text}");
            assert!(text.contains("bad thing"), "has message: {text}");
        }
        other => panic!("expected Exception, got {other:?}"),
    }
}

#[test]
fn setup_and_teardown_lifecycle() {
    let engine = engine(
        r#"
export function setup() { return { token: 'abc' }; }
export function teardown(data) { session.vars.got = data.token; }
export default function (data) { return data.token; }
"#,
    );
    let mut host = MockHost::new();
    let setup_data = engine.setup(&mut host).expect("setup");
    assert_eq!(setup_data, json!({"token": "abc"}));

    let mut vu = engine.instantiate().expect("instantiate");
    let result = vu
        .call_function(&mut host, "default", std::slice::from_ref(&setup_data))
        .expect("default");
    assert_eq!(result, json!("abc"));

    engine.teardown(&mut host, setup_data).expect("teardown");
    assert_eq!(host.vars.get("got"), Some(&json!("abc")));
}

#[test]
fn setup_defaults_to_null_when_absent() {
    let engine = engine("export default function () {}");
    let mut host = MockHost::new();
    assert_eq!(engine.setup(&mut host).expect("setup"), json!(null));
    engine
        .teardown(&mut host, json!(null))
        .expect("teardown no-op");
}

#[test]
fn named_exports_are_callable() {
    let engine = engine(
        r#"
export function beforeRequest(req) { return { tagged: req.url }; }
export function afterRequest() { return 'after'; }
export default function () {}
"#,
    );
    let mut vu = engine.instantiate().expect("instantiate");
    let mut host = MockHost::new();
    let before = vu
        .call_function(&mut host, "beforeRequest", &[json!({"url": "https://x/"})])
        .expect("beforeRequest");
    assert_eq!(before, json!({"tagged": "https://x/"}));
    let after = vu
        .call_function(&mut host, "afterRequest", &[])
        .expect("afterRequest");
    assert_eq!(after, json!("after"));
}
