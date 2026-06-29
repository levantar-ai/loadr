//! `observe` — pull system metrics in for load↔system correlation.
//!
//! The inverse of the push-based outputs: for the run window, query the
//! configured sources (Prometheus first), normalize the response into a simple
//! time-series form, and overlay it on the run timeline. A source that is
//! unreachable or returns garbage is logged and skipped — it never fails the
//! load test.
//!
//! Collection is *post-run* (one range query per source over
//! `[started_ms, ended_ms]`); live streaming into the engine snapshot loop is a
//! later phase.

use crate::http_client;
use http::{HeaderName, HeaderValue, Uri};
use loadr_config::ObserveConfig;
use loadr_core::Summary;

/// A normalized external metric series: time-ordered `(unix_ms, value)` points.
#[derive(Debug, Clone)]
pub struct ObservedSeries {
    /// Canonical metric name (e.g. `system_cpu`).
    pub name: String,
    /// Unit hint for axis formatting (`ratio`, `bytes`, …); empty if unknown.
    pub unit: String,
    /// Samples, ascending by timestamp.
    pub points: Vec<(i64, f64)>,
}

/// Pick a sensible range-query step (seconds) from the run's timeline cadence.
pub fn step_for(timeline: &[loadr_core::summary::TimelinePoint]) -> u64 {
    if timeline.len() >= 2 {
        let gap = (timeline[1].elapsed_secs - timeline[0].elapsed_secs).round();
        (gap as i64).clamp(1, 3600) as u64
    } else {
        1
    }
}

/// Collect every configured source over `[start_ms, end_ms]` at `step_secs`
/// resolution. Per-source failures are logged and skipped.
pub async fn collect(
    configs: &[ObserveConfig],
    start_ms: i64,
    end_ms: i64,
    step_secs: u64,
) -> Vec<ObservedSeries> {
    let client = http_client::client();
    let mut out = Vec::new();
    for cfg in configs {
        match cfg {
            ObserveConfig::Prometheus {
                name,
                source,
                query,
                as_name,
                unit,
                token,
            } => {
                let label = as_name
                    .clone()
                    .or_else(|| name.clone())
                    .unwrap_or_else(|| sanitize(query));
                match prometheus_range(
                    &client,
                    source,
                    query,
                    token.as_deref(),
                    start_ms,
                    end_ms,
                    step_secs,
                )
                .await
                {
                    Ok(series) => {
                        // One PromQL expr can return several label sets; suffix
                        // all but the first so names stay unique.
                        for (i, points) in series.into_iter().enumerate() {
                            let name = if i == 0 {
                                label.clone()
                            } else {
                                format!("{label}_{i}")
                            };
                            out.push(ObservedSeries {
                                name,
                                unit: unit.clone().unwrap_or_default(),
                                points,
                            });
                        }
                    }
                    Err(e) => {
                        tracing::warn!(
                            "observe: prometheus source '{}' failed: {e}",
                            name.as_deref().unwrap_or(query)
                        );
                    }
                }
            }
        }
    }
    out
}

/// Resample each series onto the run timeline (nearest sample per point) and
/// write the values into each `TimelinePoint::external`.
pub fn attach(summary: &mut Summary, series: &[ObservedSeries]) {
    if series.is_empty() || summary.timeline.is_empty() {
        return;
    }
    let start_ms = summary.started_ms as i64;
    for s in series {
        if s.points.is_empty() {
            continue;
        }
        // Tolerance: don't fill across gaps wider than ~2 sample spacings.
        let spacing = if s.points.len() >= 2 {
            (s.points[s.points.len() - 1].0 - s.points[0].0) / (s.points.len() as i64 - 1)
        } else {
            5_000
        };
        let tol = (spacing * 2).max(5_000);
        for p in &mut summary.timeline {
            let abs = start_ms + (p.elapsed_secs * 1000.0) as i64;
            if let Some(v) = nearest(&s.points, abs, tol) {
                p.external.insert(s.name.clone(), v);
            }
        }
    }
}

/// Nearest sample value to `target_ms` within `tol_ms`, else `None`.
fn nearest(points: &[(i64, f64)], target_ms: i64, tol_ms: i64) -> Option<f64> {
    let mut best: Option<(i64, f64)> = None;
    for &(ts, v) in points {
        let d = (ts - target_ms).abs();
        if d <= tol_ms && best.is_none_or(|(bd, _)| d < bd) {
            best = Some((d, v));
        }
    }
    best.map(|(_, v)| v)
}

/// Run a Prometheus range query and return one `Vec<(unix_ms, value)>` per
/// returned series.
async fn prometheus_range(
    client: &http_client::HttpClient,
    source: &str,
    query: &str,
    token: Option<&str>,
    start_ms: i64,
    end_ms: i64,
    step_secs: u64,
) -> Result<Vec<Vec<(i64, f64)>>, String> {
    let base = source.trim_end_matches('/');
    let url = format!(
        "{base}/api/v1/query_range?query={q}&start={start}&end={end}&step={step}",
        q = percent_encode(query),
        start = start_ms / 1000,
        end = end_ms / 1000,
        step = step_secs.max(1),
    );
    let uri: Uri = url.parse().map_err(|e| format!("bad url {url}: {e}"))?;

    let mut headers: Vec<(HeaderName, HeaderValue)> = Vec::new();
    if let Some(tok) = token {
        if let Ok(v) = HeaderValue::from_str(&format!("Bearer {tok}")) {
            headers.push((http::header::AUTHORIZATION, v));
        }
    }

    let (status, body) = http_client::get(client, &uri, &headers).await?;
    if !status.is_success() {
        return Err(format!("HTTP {status}"));
    }
    let json: serde_json::Value =
        serde_json::from_slice(&body).map_err(|e| format!("invalid JSON: {e}"))?;
    Ok(parse_matrix(&json))
}

/// Parse a Prometheus `query_range` matrix response into per-series points.
/// Tolerant: anything unexpected yields an empty result rather than erroring.
fn parse_matrix(json: &serde_json::Value) -> Vec<Vec<(i64, f64)>> {
    let result = match json.get("data").and_then(|d| d.get("result")) {
        Some(serde_json::Value::Array(a)) => a,
        _ => return Vec::new(),
    };
    let mut out = Vec::new();
    for entry in result {
        let Some(values) = entry.get("values").and_then(|v| v.as_array()) else {
            continue;
        };
        let mut points = Vec::with_capacity(values.len());
        for pair in values {
            let Some(arr) = pair.as_array() else { continue };
            if arr.len() != 2 {
                continue;
            }
            let ts = arr[0].as_f64();
            let val = arr[1].as_str().and_then(|s| s.parse::<f64>().ok());
            if let (Some(ts), Some(val)) = (ts, val) {
                if val.is_finite() {
                    points.push(((ts * 1000.0) as i64, val));
                }
            }
        }
        if !points.is_empty() {
            out.push(points);
        }
    }
    out
}

/// Minimal percent-encoding for a URL query component (RFC 3986 unreserved set).
fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Fall back name from a PromQL query: keep it short and identifier-ish.
fn sanitize(query: &str) -> String {
    let s: String = query
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect();
    let s = s.trim_matches('_');
    s.chars().take(40).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_matrix_response() {
        let json: serde_json::Value = serde_json::from_str(
            r#"{"status":"success","data":{"resultType":"matrix","result":[
                {"metric":{"instance":"api-1"},"values":[[1700000000,"0.12"],[1700000001,"0.40"],[1700000002,"NaN"]]}
            ]}}"#,
        )
        .unwrap();
        let series = parse_matrix(&json);
        assert_eq!(series.len(), 1);
        // NaN dropped, two finite points kept, ms-scaled.
        assert_eq!(
            series[0],
            vec![(1_700_000_000_000, 0.12), (1_700_000_001_000, 0.40)]
        );
    }

    #[test]
    fn empty_or_garbage_yields_no_series() {
        assert!(parse_matrix(&serde_json::json!({})).is_empty());
        assert!(parse_matrix(&serde_json::json!({"data":{"result":"nope"}})).is_empty());
    }

    #[test]
    fn nearest_respects_tolerance() {
        let pts = vec![(1000, 1.0), (2000, 2.0), (3000, 3.0)];
        assert_eq!(nearest(&pts, 2100, 500), Some(2.0)); // closest is 2000
        assert_eq!(nearest(&pts, 9000, 500), None); // beyond tolerance
    }

    #[test]
    fn attach_resamples_onto_timeline() {
        let mut summary = Summary {
            name: None,
            run_id: "r".into(),
            started_ms: 1_700_000_000_000,
            ended_ms: 1_700_000_003_000,
            duration_secs: 3.0,
            scenarios: vec![],
            metrics: vec![],
            checks: vec![],
            thresholds: vec![],
            thresholds_passed: true,
            aborted: None,
            snapshot: Default::default(),
            timeline: vec![tp(0.0), tp(1.0), tp(2.0)],
        };
        let series = vec![ObservedSeries {
            name: "system_cpu".into(),
            unit: "ratio".into(),
            points: vec![
                (1_700_000_000_000, 0.10),
                (1_700_000_001_000, 0.55),
                (1_700_000_002_000, 0.90),
            ],
        }];
        attach(&mut summary, &series);
        assert_eq!(summary.timeline[0].external.get("system_cpu"), Some(&0.10));
        assert_eq!(summary.timeline[1].external.get("system_cpu"), Some(&0.55));
        assert_eq!(summary.timeline[2].external.get("system_cpu"), Some(&0.90));
    }

    fn tp(elapsed: f64) -> loadr_core::summary::TimelinePoint {
        loadr_core::summary::TimelinePoint {
            elapsed_secs: elapsed,
            rps: 0.0,
            iterations_ps: 0.0,
            active_vus: 0.0,
            error_rate: 0.0,
            latency_avg: None,
            latency_p50: None,
            latency_p95: None,
            latency_p99: None,
            external: Default::default(),
        }
    }
}
