//! HAR (HTTP Archive) importer with heuristic auto-correlation.
//!
//! Browsers and proxies export captured traffic as a HAR file (JSON). This
//! converter turns one into a runnable loadr plan and — the useful part —
//! **auto-correlates** dynamic values: when a value returned in one JSON
//! response (a CSRF token, an auth token, an id) is reused in a later request,
//! it adds an `extract:` to the producing request and rewrites the literal in
//! the consumers to `${var}`. Cookies are intentionally left alone: loadr's
//! per-VU cookie jar replays `Set-Cookie` automatically.
//!
//! Everything is best-effort; anything approximated becomes a
//! [`ConversionWarning`]. The produced plan passes `loadr_config::validate`.

use std::collections::{HashMap, HashSet};

use indexmap::IndexMap;
use regex::Regex;
use serde_json::Value;

use loadr_config::{
    Body, BodySpec, ClassicExtractor, Defaults, Dur, ExecutorKind, Extractor, HttpDefaults,
    RequestStep, Scenario, Step, TestPlan,
};

use crate::{Conversion, ConversionWarning, ConvertError};

/// Minimum length for a value to be considered a correlation candidate.
const MIN_VALUE_LEN: usize = 4;

/// Request headers that are connection/transport-managed and should not be
/// replayed verbatim (loadr sets them itself, or the cookie jar handles them).
fn header_dropped(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.starts_with(':') // HTTP/2 pseudo-headers (:method, :authority, ...)
        || matches!(
            n.as_str(),
            "host" | "content-length" | "connection" | "cookie" | "accept-encoding"
        )
}

/// True for responses that are static assets (noise in a load test).
fn is_static_asset(mime: &str, url_path: &str) -> bool {
    let m = mime.to_ascii_lowercase();
    if m.starts_with("image/")
        || m.starts_with("font/")
        || m.starts_with("audio/")
        || m.starts_with("video/")
        || m.contains("css")
        || m.contains("javascript")
    {
        return true;
    }
    let path = url_path
        .split('?')
        .next()
        .unwrap_or(url_path)
        .to_ascii_lowercase();
    const EXT: &[&str] = &[
        ".css", ".js", ".mjs", ".png", ".jpg", ".jpeg", ".gif", ".svg", ".webp", ".ico", ".woff",
        ".woff2", ".ttf", ".eot", ".map", ".avif",
    ];
    EXT.iter().any(|e| path.ends_with(e))
}

/// Split an absolute URL into `(origin, path_and_query)`.
fn split_origin(url: &str) -> Option<(String, String)> {
    let scheme_end = url.find("://")?;
    let after = &url[scheme_end + 3..];
    let path_start = after.find('/').unwrap_or(after.len());
    let origin = url[..scheme_end + 3 + path_start].to_string();
    let rest = &after[path_start..];
    let pq = if rest.is_empty() {
        "/".to_string()
    } else {
        rest.to_string()
    };
    Some((origin, pq))
}

/// Compiled dynamic-value patterns.
struct Patterns {
    uuid: Regex,
    hex16: Regex,
    numeric_id: Regex,
    token: Regex,
}

impl Patterns {
    fn new() -> Patterns {
        Patterns {
            uuid: Regex::new(
                r"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$",
            )
            .unwrap(),
            hex16: Regex::new(r"^[0-9a-fA-F]{16,}$").unwrap(),
            numeric_id: Regex::new(r"^[0-9]{4,}$").unwrap(),
            token: Regex::new(r"^[A-Za-z0-9_\-.]{16,}$").unwrap(),
        }
    }
    fn value_looks_dynamic(&self, v: &str) -> bool {
        self.uuid.is_match(v)
            || (v.starts_with("eyJ") && v.contains('.')) // JWT
            || self.hex16.is_match(v)
            || self.numeric_id.is_match(v)
            || self.token.is_match(v)
    }
}

/// True if a JSON key name suggests a security/identity value worth correlating.
fn key_is_sensitive(key: &str) -> bool {
    let k = key.to_ascii_lowercase();
    const TERMS: &[&str] = &[
        "token", "csrf", "xsrf", "nonce", "session", "secret", "auth", "ticket", "sid", "uuid",
        "guid",
    ];
    TERMS.iter().any(|t| k.contains(t)) || k == "id" || k.ends_with("id") || k.ends_with("_id")
}

/// Values that are never worth correlating even if they pass other tests.
fn value_denied(v: &str) -> bool {
    matches!(
        v,
        "true" | "false" | "null" | "none" | "undefined" | "application/json" | "text/html"
    )
}

/// Whether `(key, value)` is a correlation candidate.
fn is_candidate(key: &str, value: &str, pats: &Patterns) -> bool {
    value.len() >= MIN_VALUE_LEN
        && !value.chars().any(char::is_whitespace)
        && !value_denied(value)
        && (key_is_sensitive(key) || pats.value_looks_dynamic(value))
}

/// A value produced by a response that may be reused later.
struct Candidate {
    producer: usize,
    jsonpath: String,
    value: String,
    key: String,
}

/// Walk a JSON value, collecting candidate leaves with their JSONPath.
fn collect_candidates(
    value: &Value,
    path: &str,
    key: &str,
    pats: &Patterns,
    out: &mut Vec<(String, String, String)>, // (jsonpath, value, key)
) {
    match value {
        Value::Object(map) => {
            for (k, v) in map {
                collect_candidates(v, &format!("{path}.{k}"), k, pats, out);
            }
        }
        Value::Array(arr) => {
            for (i, v) in arr.iter().enumerate() {
                collect_candidates(v, &format!("{path}[{i}]"), key, pats, out);
            }
        }
        Value::String(s) => {
            if is_candidate(key, s, pats) {
                out.push((path.to_string(), s.clone(), key.to_string()));
            }
        }
        Value::Number(n) => {
            let s = n.to_string();
            if is_candidate(key, &s, pats) {
                out.push((path.to_string(), s, key.to_string()));
            }
        }
        _ => {}
    }
}

/// Turn a JSON key into a unique snake_case variable name.
fn var_name(key: &str, used: &mut HashSet<String>) -> String {
    let mut s: String = key
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    while s.contains("__") {
        s = s.replace("__", "_");
    }
    let mut base = s.trim_matches('_').to_string();
    if base.is_empty() || !base.chars().next().unwrap().is_ascii_alphabetic() {
        base = format!("v_{base}");
        base = base.trim_matches('_').to_string();
    }
    if base.is_empty() {
        base = "var".to_string();
    }
    let mut name = base.clone();
    let mut n = 2;
    while used.contains(&name) {
        name = format!("{base}_{n}");
        n += 1;
    }
    used.insert(name.clone());
    name
}

/// A request being assembled. Correlation is plain string replacement over
/// `url` / header values / `body_text`, which keeps it simple and lossless.
struct RawReq {
    method: String,
    url: String,
    headers: Vec<(String, String)>,
    body_text: Option<String>,
    body_is_json: bool,
    response_json: Option<Value>,
    extract: Vec<Extractor>,
}

impl RawReq {
    fn contains(&self, value: &str) -> bool {
        self.url.contains(value)
            || self.headers.iter().any(|(_, v)| v.contains(value))
            || self.body_text.as_deref().is_some_and(|b| b.contains(value))
    }

    fn replace(&mut self, value: &str, repl: &str) {
        if self.url.contains(value) {
            self.url = self.url.replace(value, repl);
        }
        for (_, v) in self.headers.iter_mut() {
            if v.contains(value) {
                *v = v.replace(value, repl);
            }
        }
        if let Some(b) = &mut self.body_text {
            if b.contains(value) {
                *b = b.replace(value, repl);
            }
        }
    }
}

fn header_array(v: Option<&Value>) -> Vec<(String, String)> {
    v.and_then(|h| h.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|h| {
                    let name = h.get("name")?.as_str()?;
                    let val = h.get("value")?.as_str()?;
                    Some((name.to_string(), val.to_string()))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Convert a HAR 1.2 document into a loadr [`TestPlan`].
pub fn convert_har(source: &str) -> Result<Conversion, ConvertError> {
    let root: Value = serde_json::from_str(source)
        .map_err(|e| ConvertError::Har(format!("not valid JSON: {e}")))?;
    let entries = root
        .get("log")
        .and_then(|l| l.get("entries"))
        .and_then(|e| e.as_array())
        .ok_or_else(|| ConvertError::Har("missing `log.entries` (not a HAR file?)".into()))?;

    let pats = Patterns::new();
    let mut warnings = Vec::new();
    let mut reqs: Vec<RawReq> = Vec::new();
    let mut origin_counts: HashMap<String, usize> = HashMap::new();
    let mut skipped_static = 0usize;

    for entry in entries {
        let request = match entry.get("request") {
            Some(r) => r,
            None => continue,
        };
        let method = request
            .get("method")
            .and_then(|m| m.as_str())
            .unwrap_or("GET")
            .to_uppercase();
        let url = match request.get("url").and_then(|u| u.as_str()) {
            Some(u) => u.to_string(),
            None => continue,
        };

        let response = entry.get("response");
        let resp_mime = response
            .and_then(|r| r.get("content"))
            .and_then(|c| c.get("mimeType"))
            .and_then(|m| m.as_str())
            .unwrap_or("");
        let (_, path_for_check) = split_origin(&url).unwrap_or_default();
        if is_static_asset(resp_mime, &path_for_check) {
            skipped_static += 1;
            continue;
        }

        if let Some((origin, _)) = split_origin(&url) {
            *origin_counts.entry(origin).or_insert(0) += 1;
        }

        // Headers (minus transport/cookie noise).
        let headers: Vec<(String, String)> = header_array(request.get("headers"))
            .into_iter()
            .filter(|(n, _)| !header_dropped(n))
            .collect();

        // Request body.
        let post = request.get("postData");
        let body_text = post
            .and_then(|p| p.get("text"))
            .and_then(|t| t.as_str())
            .map(|s| s.to_string());
        let req_mime = post
            .and_then(|p| p.get("mimeType"))
            .and_then(|m| m.as_str())
            .unwrap_or("");
        let body_is_json = req_mime.contains("json")
            && body_text
                .as_deref()
                .is_some_and(|t| serde_json::from_str::<Value>(t).is_ok());

        // Parse the response JSON body (for correlation sources).
        let response_json = if resp_mime.contains("json") {
            response
                .and_then(|r| r.get("content"))
                .and_then(|c| c.get("text"))
                .and_then(|t| t.as_str())
                .and_then(|t| serde_json::from_str::<Value>(t).ok())
        } else {
            None
        };

        reqs.push(RawReq {
            method,
            url,
            headers,
            body_text,
            body_is_json,
            response_json,
            extract: Vec::new(),
        });
    }

    if reqs.is_empty() {
        return Err(ConvertError::Har(
            "no non-static HTTP requests found in the HAR".into(),
        ));
    }

    // Base URL: the most common origin. Relativise matching requests so the
    // plan reads cleanly and correlation matches on path+query.
    let base_url = origin_counts
        .iter()
        .max_by_key(|(_, c)| **c)
        .map(|(o, _)| o.clone());
    if let Some(base) = &base_url {
        for r in reqs.iter_mut() {
            if let Some((origin, pq)) = split_origin(&r.url) {
                if &origin == base {
                    r.url = pq;
                }
            }
        }
    }

    // ---- correlation pass -------------------------------------------------
    // Collect candidates in document order, earliest producer wins per value.
    let mut seen: HashSet<String> = HashSet::new();
    let mut candidates: Vec<Candidate> = Vec::new();
    for (i, r) in reqs.iter().enumerate() {
        if let Some(body) = &r.response_json {
            let mut leaves = Vec::new();
            collect_candidates(body, "$", "", &pats, &mut leaves);
            for (jsonpath, value, key) in leaves {
                if seen.insert(value.clone()) {
                    candidates.push(Candidate {
                        producer: i,
                        jsonpath,
                        value,
                        key,
                    });
                }
            }
        }
    }

    let mut used_names: HashSet<String> = HashSet::new();
    let mut correlated = 0usize;
    for cand in &candidates {
        // Which later requests reuse this value?
        let consumers: Vec<usize> = (cand.producer + 1..reqs.len())
            .filter(|&j| reqs[j].contains(&cand.value))
            .collect();
        if consumers.is_empty() {
            continue;
        }
        let name = var_name(&cand.key, &mut used_names);
        reqs[cand.producer]
            .extract
            .push(Extractor::Classic(ClassicExtractor::Jsonpath {
                name: name.clone(),
                expression: cand.jsonpath.clone(),
                default: None,
                index: None,
            }));
        let repl = format!("${{{name}}}");
        for &j in &consumers {
            reqs[j].replace(&cand.value, &repl);
        }
        correlated += 1;
        warnings.push(ConversionWarning {
            element: format!("request #{}", cand.producer + 1),
            message: format!(
                "auto-correlated `{name}` ({} from the response) into {} later request(s) — review it",
                cand.jsonpath,
                consumers.len()
            ),
        });
    }

    // ---- build the plan ---------------------------------------------------
    let flow: Vec<Step> = reqs
        .into_iter()
        .map(|r| {
            let mut headers = IndexMap::new();
            for (n, v) in r.headers {
                headers.insert(n, v);
            }
            let body = r.body_text.map(|t| {
                if r.body_is_json {
                    match serde_json::from_str::<Value>(&t) {
                        Ok(json) => Body::Spec(BodySpec {
                            json: Some(json),
                            ..Default::default()
                        }),
                        Err(_) => Body::Text(t),
                    }
                } else {
                    Body::Text(t)
                }
            });
            let path = r.url.split('?').next().unwrap_or(&r.url).to_string();
            Step::Request(Box::new(RequestStep {
                name: Some(format!("{} {}", r.method, path)),
                method: Some(r.method),
                url: r.url,
                headers,
                body,
                extract: r.extract,
                ..Default::default()
            }))
        })
        .collect();

    let scenario = Scenario {
        executor: ExecutorKind::ConstantVus,
        vus: Some(1),
        duration: Some(Dur::from_secs(60)),
        flow,
        ..Default::default()
    };
    let mut scenarios = IndexMap::new();
    scenarios.insert("recorded".to_string(), scenario);

    let plan = TestPlan {
        name: Some("recorded session".to_string()),
        description: Some(
            "Imported from a HAR recording by `loadr convert`. Review correlations and set real load.".to_string(),
        ),
        defaults: Defaults {
            http: HttpDefaults {
                base_url,
                ..Default::default()
            },
            ..Default::default()
        },
        scenarios,
        ..Default::default()
    };

    if skipped_static > 0 {
        warnings.insert(
            0,
            ConversionWarning {
                element: "static assets".to_string(),
                message: format!(
                    "skipped {skipped_static} static-asset request(s) (images, CSS, JS, fonts)"
                ),
            },
        );
    }
    warnings.push(ConversionWarning {
        element: "scenario `recorded`".to_string(),
        message: "defaulted to constant-vus 1 VU for 60s — set real vus/duration/executor before load testing".to_string(),
    });
    if correlated == 0 {
        warnings.push(ConversionWarning {
            element: "correlation".to_string(),
            message: "no dynamic values were auto-correlated (no JSON response value was reused in a later request)".to_string(),
        });
    }

    Ok(Conversion { plan, warnings })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
    {
      "log": {
        "version": "1.2",
        "entries": [
          {
            "request": { "method": "GET", "url": "https://app.example.com/styles/main.css", "headers": [] },
            "response": { "status": 200, "content": { "mimeType": "text/css", "text": "body{}" } }
          },
          {
            "request": { "method": "GET", "url": "https://app.example.com/logo.png", "headers": [] },
            "response": { "status": 200, "content": { "mimeType": "image/png", "text": "" } }
          },
          {
            "request": {
              "method": "POST",
              "url": "https://app.example.com/api/login",
              "headers": [
                { "name": "Host", "value": "app.example.com" },
                { "name": "Content-Type", "value": "application/json" },
                { "name": "Cookie", "value": "sessionx=old" }
              ],
              "postData": { "mimeType": "application/json", "text": "{\"user\":\"alice\"}" }
            },
            "response": {
              "status": 200,
              "content": {
                "mimeType": "application/json",
                "text": "{\"csrfToken\":\"3f2504e0-4f89-41d3-9a0c-0305e82c3301\",\"user\":{\"id\":987654}}"
              }
            }
          },
          {
            "request": {
              "method": "POST",
              "url": "https://app.example.com/api/orders",
              "headers": [
                { "name": "Content-Type", "value": "application/json" },
                { "name": "X-CSRF-Token", "value": "3f2504e0-4f89-41d3-9a0c-0305e82c3301" }
              ],
              "postData": { "mimeType": "application/json", "text": "{\"token\":\"3f2504e0-4f89-41d3-9a0c-0305e82c3301\",\"qty\":1}" }
            },
            "response": { "status": 201, "content": { "mimeType": "application/json", "text": "{\"ok\":true}" } }
          },
          {
            "request": {
              "method": "GET",
              "url": "https://app.example.com/api/users/987654/profile",
              "headers": []
            },
            "response": { "status": 200, "content": { "mimeType": "application/json", "text": "{\"name\":\"alice\"}" } }
          }
        ]
      }
    }
    "#;

    fn convert() -> Conversion {
        convert_har(SAMPLE).expect("conversion")
    }

    #[test]
    fn rejects_non_har() {
        assert!(matches!(convert_har("{}"), Err(ConvertError::Har(_))));
        assert!(matches!(convert_har("not json"), Err(ConvertError::Har(_))));
    }

    #[test]
    fn skips_static_assets_and_sets_base_url() {
        let c = convert();
        let scenario = c.plan.scenarios.get("recorded").unwrap();
        // 5 entries, 2 static (css/png) dropped → 3 requests.
        assert_eq!(scenario.flow.len(), 3, "should keep 3 non-static requests");
        assert_eq!(
            c.plan.defaults.http.base_url.as_deref(),
            Some("https://app.example.com")
        );
        assert!(c
            .warnings
            .iter()
            .any(|w| w.message.contains("skipped 2 static-asset")));
    }

    fn request(c: &Conversion, i: usize) -> &RequestStep {
        match &c.plan.scenarios.get("recorded").unwrap().flow[i] {
            Step::Request(r) => r,
            _ => panic!("expected request step"),
        }
    }

    #[test]
    fn correlates_csrf_token_and_id() {
        let c = convert();

        // Producer (login) gained extractors for the token and the id.
        let login = request(&c, 0);
        assert_eq!(login.url, "/api/login");
        let exprs: Vec<&str> = login
            .extract
            .iter()
            .filter_map(|e| match e {
                Extractor::Classic(ClassicExtractor::Jsonpath { expression, .. }) => {
                    Some(expression.as_str())
                }
                _ => None,
            })
            .collect();
        assert!(
            exprs.iter().any(|e| e.contains("csrfToken")),
            "should extract csrfToken, got {exprs:?}"
        );
        assert!(
            exprs.contains(&"$.user.id"),
            "should extract $.user.id, got {exprs:?}"
        );

        // Consumer (orders): the literal uuid is gone, replaced by a ${var} in
        // both the header and the JSON body.
        let orders = request(&c, 1);
        let header_val = orders
            .headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("X-CSRF-Token"))
            .map(|(_, v)| v.as_str())
            .unwrap();
        assert!(
            header_val.starts_with("${") && header_val.ends_with('}'),
            "csrf header should be a ${{var}}, got {header_val}"
        );
        let body = match orders.body.as_ref().unwrap() {
            Body::Spec(b) => serde_json::to_string(b.json.as_ref().unwrap()).unwrap(),
            Body::Text(t) => t.clone(),
        };
        assert!(
            !body.contains("3f2504e0-4f89-41d3-9a0c-0305e82c3301"),
            "literal token must be gone from the body, got {body}"
        );
        assert!(body.contains("${"), "body should reference a ${{var}}");

        // Consumer (profile GET): the id in the URL path was correlated.
        let profile = request(&c, 2);
        assert!(
            !profile.url.contains("987654"),
            "literal id must be gone from the URL, got {}",
            profile.url
        );
        assert!(
            profile.url.contains("${"),
            "url should reference a ${{var}}, got {}",
            profile.url
        );
    }

    #[test]
    fn drops_transport_headers() {
        let c = convert();
        let login = request(&c, 0);
        for (k, _) in &login.headers {
            let k = k.to_ascii_lowercase();
            assert!(k != "host" && k != "cookie", "transport header leaked: {k}");
        }
        // Content-Type is kept.
        assert!(login
            .headers
            .iter()
            .any(|(k, _)| k.eq_ignore_ascii_case("content-type")));
    }

    #[test]
    fn plan_passes_validation() {
        let c = convert();
        let yaml = serde_yaml::to_string(&c.plan).unwrap();
        // Round-trips through the schema and validates without errors.
        let opts = loadr_config::LoadOptions {
            check_files: false,
            deny_errors: true,
            ..Default::default()
        };
        loadr_config::load_str(&yaml, &opts).expect("converted plan should be valid");
    }
}
