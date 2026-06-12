//! Compiled thresholds and their continuous evaluation.

use std::time::Duration;

use loadr_config::{Agg, MetricSelector, ThresholdExpr, ThresholdList};

use crate::aggregate::Aggregator;
use crate::metrics::MetricKind;

/// One compiled threshold.
#[derive(Debug)]
pub struct CompiledThreshold {
    pub selector: MetricSelector,
    pub expr: ThresholdExpr,
    pub abort_on_fail: bool,
    pub delay_abort_eval: Option<Duration>,
    /// Original expression text for display.
    pub source: String,
}

/// Result of evaluating one threshold.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ThresholdStatus {
    pub metric: String,
    pub expression: String,
    /// The observed aggregate value, when samples exist.
    pub observed: Option<f64>,
    pub passed: bool,
    pub abort_on_fail: bool,
}

/// Compile the `thresholds:` block.
pub fn compile_thresholds(
    thresholds: &indexmap::IndexMap<String, ThresholdList>,
) -> Result<Vec<CompiledThreshold>, String> {
    let mut out = Vec::new();
    for (selector_str, list) in thresholds {
        let selector = MetricSelector::parse(selector_str).map_err(|e| e.to_string())?;
        for entry in list.entries() {
            let expr = ThresholdExpr::parse(entry.expression())
                .map_err(|e| format!("threshold `{selector_str}`: {e}"))?;
            out.push(CompiledThreshold {
                selector: selector.clone(),
                expr,
                abort_on_fail: entry.abort_on_fail(),
                delay_abort_eval: entry.delay_abort_eval().map(|d| d.as_duration()),
                source: entry.expression().to_string(),
            });
        }
    }
    Ok(out)
}

/// Evaluate one threshold against the aggregator.
/// A threshold with no samples yet passes (matching k6).
pub fn evaluate(threshold: &CompiledThreshold, agg: &Aggregator) -> ThresholdStatus {
    let metric = &threshold.selector.metric;
    let observed = agg
        .aggregate_selector(metric, &threshold.selector.tags)
        .and_then(|(kind, values)| {
            match threshold.expr.agg {
                // Arbitrary percentiles fall back to a merged-histogram query.
                Agg::Percentile(p) => values.value_for(&threshold.expr.agg, kind).or_else(|| {
                    agg.aggregate_selector_percentile(metric, &threshold.selector.tags, p)
                }),
                _ => values.value_for(&threshold.expr.agg, kind),
            }
        });
    let passed = match observed {
        Some(v) => threshold.expr.op.eval(v, threshold.expr.bound),
        None => true,
    };
    ThresholdStatus {
        metric: threshold.selector.to_string(),
        expression: threshold.source.clone(),
        observed,
        passed,
        abort_on_fail: threshold.abort_on_fail,
    }
}

/// Evaluate all thresholds; second element is true when an abort-on-fail
/// threshold (past its delay) is failing.
pub fn evaluate_all(
    thresholds: &[CompiledThreshold],
    agg: &Aggregator,
    elapsed: Duration,
) -> (Vec<ThresholdStatus>, bool) {
    let mut abort = false;
    let statuses: Vec<ThresholdStatus> = thresholds
        .iter()
        .map(|t| {
            let status = evaluate(t, agg);
            if !status.passed && t.abort_on_fail {
                let delay_ok = t.delay_abort_eval.map(|d| elapsed >= d).unwrap_or(true);
                if delay_ok {
                    abort = true;
                }
            }
            status
        })
        .collect();
    (statuses, abort)
}

/// Helper for summary display: the metric kind a threshold applies to.
pub fn threshold_kind(agg: &Aggregator, t: &CompiledThreshold) -> Option<MetricKind> {
    agg.aggregate_selector(&t.selector.metric, &t.selector.tags)
        .map(|(k, _)| k)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::{now_millis, Sample, Tags};
    use std::sync::Arc;

    fn agg_with_durations(values: &[f64]) -> Aggregator {
        let mut agg = Aggregator::new();
        for v in values {
            agg.record(&Sample {
                metric: Arc::from("http_req_duration"),
                kind: MetricKind::Trend,
                value: *v,
                tags: Arc::new(Tags::new()),
                timestamp_ms: now_millis(),
            });
        }
        agg
    }

    fn compile(yaml: &str) -> Vec<CompiledThreshold> {
        let map: indexmap::IndexMap<String, ThresholdList> =
            serde_yaml::from_str(yaml).expect("yaml");
        compile_thresholds(&map).expect("compile")
    }

    #[test]
    fn passing_and_failing() {
        let agg = agg_with_durations(&(1..=100).map(|i| i as f64).collect::<Vec<_>>());
        let ts = compile(r#"{ http_req_duration: [ "p(95)<200", "avg<10" ] }"#);
        let (statuses, abort) = evaluate_all(&ts, &agg, Duration::from_secs(10));
        assert!(statuses[0].passed, "{:?}", statuses[0]);
        assert!(!statuses[1].passed, "{:?}", statuses[1]);
        assert!(!abort, "no abort_on_fail set");
    }

    #[test]
    fn abort_on_fail_with_delay() {
        let agg = agg_with_durations(&[100.0, 200.0]);
        let ts = compile(
            r#"{ http_req_duration: [ { threshold: "max<50", abort_on_fail: true, delay_abort_eval: 30s } ] }"#,
        );
        let (_, abort_early) = evaluate_all(&ts, &agg, Duration::from_secs(5));
        assert!(!abort_early, "within delay window");
        let (_, abort_late) = evaluate_all(&ts, &agg, Duration::from_secs(31));
        assert!(abort_late, "past delay window");
    }

    #[test]
    fn no_samples_passes() {
        let agg = Aggregator::new();
        let ts = compile(r#"{ http_req_duration: [ "p(95)<200" ] }"#);
        let (statuses, abort) = evaluate_all(&ts, &agg, Duration::ZERO);
        assert!(statuses[0].passed);
        assert!(statuses[0].observed.is_none());
        assert!(!abort);
    }

    #[test]
    fn arbitrary_percentile() {
        let agg = agg_with_durations(&(1..=1000).map(|i| i as f64).collect::<Vec<_>>());
        let ts = compile(r#"{ http_req_duration: [ "p(42)<500" ] }"#);
        let (statuses, _) = evaluate_all(&ts, &agg, Duration::ZERO);
        let observed = statuses[0].observed.expect("p42");
        assert!((observed - 420.0).abs() / 420.0 < 0.02, "p42={observed}");
        assert!(statuses[0].passed);
    }

    #[test]
    fn rate_metric_threshold() {
        let mut agg = Aggregator::new();
        for i in 0..100 {
            agg.record(&Sample {
                metric: Arc::from("checks"),
                kind: MetricKind::Rate,
                value: if i < 97 { 1.0 } else { 0.0 },
                tags: Arc::new(Tags::new()),
                timestamp_ms: now_millis(),
            });
        }
        let ts = compile(r#"{ checks: [ "rate>0.95" ] }"#);
        let (statuses, _) = evaluate_all(&ts, &agg, Duration::ZERO);
        assert!(statuses[0].passed);
        let ts = compile(r#"{ checks: [ "rate>0.99" ] }"#);
        let (statuses, _) = evaluate_all(&ts, &agg, Duration::ZERO);
        assert!(!statuses[0].passed);
    }

    #[test]
    fn tag_selector_threshold() {
        let mut agg = Aggregator::new();
        let mut tags_a = Tags::new();
        tags_a.insert("scenario".into(), "a".into());
        let mut tags_b = Tags::new();
        tags_b.insert("scenario".into(), "b".into());
        for (tags, value) in [(tags_a, 10.0), (tags_b, 1000.0)] {
            let tags = Arc::new(tags);
            for _ in 0..10 {
                agg.record(&Sample {
                    metric: Arc::from("http_req_duration"),
                    kind: MetricKind::Trend,
                    value,
                    tags: tags.clone(),
                    timestamp_ms: now_millis(),
                });
            }
        }
        let ts = compile(r#"{ "http_req_duration{scenario:a}": [ "avg<100" ] }"#);
        let (statuses, _) = evaluate_all(&ts, &agg, Duration::ZERO);
        assert!(statuses[0].passed, "{statuses:?}");
        let ts = compile(r#"{ http_req_duration: [ "avg<100" ] }"#);
        let (statuses, _) = evaluate_all(&ts, &agg, Duration::ZERO);
        assert!(!statuses[0].passed, "merged includes slow scenario");
    }
}
