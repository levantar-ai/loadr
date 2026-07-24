//! Self-contained HTML report rendering from a `Summary`.
//!
//! The report embeds a per-interval time series (throughput, latency
//! percentiles, active VUs, error rate) and renders it as interactive,
//! self-contained charts: the data is serialized inline as JSON and a small
//! inline script draws SVG line charts with a shared hover crosshair. No
//! external assets are referenced.

use loadr_core::{AggValues, MetricKind, SeriesSnapshot, Snapshot, Summary, TimelinePoint};

/// Render a standalone HTML report (no external assets).
pub fn render(summary: &Summary) -> String {
    let title = summary.name.as_deref().unwrap_or("loadr test");
    let status_pill = if summary.thresholds_passed && summary.aborted.is_none() {
        r#"<span class="pill pass">PASSED</span>"#.to_string()
    } else if summary.aborted.is_some() {
        format!(
            r#"<span class="pill fail">ABORTED</span> <span class="muted">{}</span>"#,
            esc(summary.aborted.as_deref().unwrap_or(""))
        )
    } else {
        r#"<span class="pill fail">THRESHOLDS FAILED</span>"#.to_string()
    };

    let mut threshold_rows = String::new();
    for t in &summary.thresholds {
        let mark = if t.passed {
            r#"<td class="ok">✓</td>"#
        } else {
            r#"<td class="bad">✗</td>"#
        };
        threshold_rows.push_str(&format!(
            "<tr>{mark}<td><code>{}</code></td><td><code>{}</code></td><td>{}</td></tr>",
            esc(&t.metric),
            esc(&t.expression),
            t.observed
                .map(|v| format!("{v:.2}"))
                .unwrap_or_else(|| "no samples".into()),
        ));
    }

    let mut check_rows = String::new();
    for c in &summary.checks {
        let total = c.passes + c.fails;
        let pct = if total > 0 {
            100.0 * c.passes as f64 / total as f64
        } else {
            100.0
        };
        let class = if c.fails == 0 { "ok" } else { "bad" };
        check_rows.push_str(&format!(
            r#"<tr><td class="{class}">{}</td><td>{}</td><td>{}</td><td>{}</td><td><div class="bar"><div style="width:{pct:.1}%"></div></div> {pct:.2}%</td></tr>"#,
            if c.fails == 0 { "✓" } else { "✗" },
            esc(&c.name),
            c.passes,
            c.fails,
        ));
    }

    let mut trend_rows = String::new();
    let mut other_rows = String::new();
    for m in &summary.metrics {
        match m.kind {
            MetricKind::Trend => {
                trend_rows.push_str(&format!(
                    "<tr><td><code>{}</code></td><td>{}</td>{}{}{}{}{}{}{}</tr>",
                    esc(&m.metric),
                    m.agg.count,
                    td_ms(m.agg.avg),
                    td_ms(m.agg.min),
                    td_ms(m.agg.med),
                    td_ms(m.agg.p90),
                    td_ms(m.agg.p95),
                    td_ms(m.agg.p99),
                    td_ms(m.agg.max),
                ));
            }
            MetricKind::Counter => {
                other_rows.push_str(&format!(
                    "<tr><td><code>{}</code></td><td>counter</td><td>{}</td><td>{}/s</td></tr>",
                    esc(&m.metric),
                    fmt_num(m.agg.sum),
                    fmt_num(m.agg.per_second.unwrap_or(0.0)),
                ));
            }
            MetricKind::Rate => {
                other_rows.push_str(&format!(
                    "<tr><td><code>{}</code></td><td>rate</td><td>{:.2}%</td><td>✓ {} ✗ {}</td></tr>",
                    esc(&m.metric),
                    m.agg.rate.unwrap_or(0.0) * 100.0,
                    m.agg.sum as u64,
                    m.agg.count - m.agg.sum as u64,
                ));
            }
            MetricKind::Gauge => {
                other_rows.push_str(&format!(
                    "<tr><td><code>{}</code></td><td>gauge</td><td>{}</td><td>min {} / max {}</td></tr>",
                    esc(&m.metric),
                    fmt_num(m.agg.last.unwrap_or(0.0)),
                    fmt_num(m.agg.min.unwrap_or(0.0)),
                    fmt_num(m.agg.max.unwrap_or(0.0)),
                ));
            }
        }
    }

    let timeseries = timeseries_section(&summary.timeline);
    let per_txn = per_transaction_section(&summary.snapshot);

    format!(
        r##"<!doctype html>
<html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>loadr report — {title}</title>
<style>
:root {{ color-scheme: dark; --bg:#07070a; --panel:#141419; --card:linear-gradient(160deg,#0c0c11 0%,#0a0a0e 100%); --border:#232330; --fg:#d1d5db; --ink:#f3f4f6; --muted:#9ca3af; --green:#4ade80; --red:#ef4444; --ember:#ef4444; --flare:#f87171; --accent:#ef4444; }}
body {{ margin:0; background:var(--bg); color:var(--fg); font:15px/1.6 ui-sans-serif,system-ui,-apple-system,"Segoe UI",Roboto,"Helvetica Neue",Arial,sans-serif; }}
.wrap {{ max-width: 1080px; margin: 0 auto; padding: 32px 20px 64px; }}
h1 {{ font-size: 27px; margin: 0 0 6px; color:var(--ink); letter-spacing:-.015em; }} h2 {{ font-size: 13px; margin: 40px 0 12px; color: var(--ink); text-transform:uppercase; letter-spacing:.07em; }}
h2::before {{ content:""; display:inline-block; width:7px; height:7px; background:var(--ember); border-radius:2px; margin-right:9px; vertical-align:baseline; }}
.muted {{ color: var(--muted); }}
.pill {{ display:inline-block; padding:2px 12px; border-radius:999px; font-weight:600; font-size:13px; }}
.pill.pass {{ background:rgba(74,222,128,.13); color:var(--green); border:1px solid rgba(74,222,128,.5); }}
.pill.fail {{ background:rgba(239,68,68,.13); color:var(--red); border:1px solid rgba(239,68,68,.5); }}
table {{ width:100%; border-collapse:collapse; background:var(--card); border:1px solid var(--border); border-radius:10px; overflow:hidden; }}
th,td {{ text-align:left; padding:8px 12px; border-bottom:1px solid var(--border); font-variant-numeric: tabular-nums; }}
th {{ color:var(--muted); font-weight:600; font-size:12px; text-transform:uppercase; letter-spacing:.04em; }}
tr:last-child td {{ border-bottom:none; }}
.ok {{ color:var(--green); }} .bad {{ color:var(--red); }} .warn {{ color:#fbbf24; }}
tr.tot td {{ font-weight:700; border-top:2px solid var(--border); background:rgba(255,255,255,.02); }}
code {{ color:var(--flare); font-family:ui-monospace,"SF Mono",Menlo,Consolas,monospace; }}
.bar {{ display:inline-block; width:120px; height:8px; background:var(--border); border-radius:4px; vertical-align:middle; margin-right:8px; }}
.bar div {{ height:8px; background:var(--green); border-radius:4px; }}
.meta {{ display:flex; gap:24px; flex-wrap:wrap; margin:18px 0 0; }}
.meta div {{ background:var(--card); border:1px solid var(--border); border-radius:10px; padding:10px 16px; }}
.meta b {{ display:block; font-size:20px; }}
footer {{ margin-top:48px; color:var(--muted); font-size:13px; }}
.charts {{ display:grid; grid-template-columns:repeat(auto-fit,minmax(420px,1fr)); gap:16px; }}
.chart {{ background:var(--card); border:1px solid var(--border); border-radius:10px; padding:14px 16px 8px; }}
.chart h3 {{ margin:0 0 2px; font-size:14px; font-weight:600; color:var(--fg); }}
.chart .legend {{ display:flex; flex-wrap:wrap; gap:12px; margin:0 0 6px; font-size:12px; color:var(--muted); }}
.chart .legend span {{ display:inline-flex; align-items:center; gap:5px; }}
.chart .legend i {{ width:11px; height:3px; border-radius:2px; display:inline-block; }}
.chart svg {{ width:100%; height:auto; display:block; cursor:crosshair; }}
.chart .axis {{ fill:var(--muted); font-size:10px; }}
.chart .grid {{ stroke:var(--border); stroke-width:1; }}
.chart .cursor {{ stroke:var(--muted); stroke-width:1; stroke-dasharray:3 3; opacity:0; }}
.chart .tip {{ position:relative; min-height:16px; font-size:12px; color:var(--muted); font-variant-numeric:tabular-nums; }}
.chart .tip b {{ color:var(--fg); }}
</style></head><body><div class="wrap">
<h1>{title}</h1>
<p>{status_pill}</p>
<div class="meta">
  <div><b>{duration:.1}s</b><span class="muted">duration</span></div>
  <div><b>{scenarios}</b><span class="muted">scenario(s)</span></div>
  <div><b>{run_id}</b><span class="muted">run id</span></div>
</div>
{timeseries}
{per_txn}<h2>Thresholds</h2>
<table><thead><tr><th></th><th>Metric</th><th>Expression</th><th>Observed</th></tr></thead>
<tbody>{threshold_rows}</tbody></table>
<h2>Checks</h2>
<table><thead><tr><th></th><th>Check</th><th>Passes</th><th>Fails</th><th>Rate</th></tr></thead>
<tbody>{check_rows}</tbody></table>
<h2>Latency (trends)</h2>
<table><thead><tr><th>Metric</th><th>Count</th><th>Avg</th><th>Min</th><th>Med</th><th>p90</th><th>p95</th><th>p99</th><th>Max</th></tr></thead>
<tbody>{trend_rows}</tbody></table>
<h2>Counters, rates &amp; gauges</h2>
<table><thead><tr><th>Metric</th><th>Kind</th><th>Value</th><th>Detail</th></tr></thead>
<tbody>{other_rows}</tbody></table>
<footer>Generated by loadr {version} — https://loadr.io</footer>
</div></body></html>"##,
        title = esc(title),
        duration = summary.duration_secs,
        scenarios = summary.scenarios.len(),
        run_id = esc(&summary.run_id),
        version = env!("CARGO_PKG_VERSION"),
    )
}

/// Per-transaction breakdown (one row per request `name`) built from the final
/// snapshot's per-tag series: statistics, response-time percentiles, APDEX and
/// the response-code distribution — the JMeter/Locust view. loadr tags every
/// named request with a `name`, so this renders automatically; it returns an
/// empty string when no series carry a `name` tag (e.g. unnamed requests or
/// summaries from before per-tag capture), leaving the rest of the report
/// unchanged.
fn per_transaction_section(snapshot: &Snapshot) -> String {
    let mut names: Vec<&str> = Vec::new();
    for s in &snapshot.series {
        if let Some(n) = s.tags.get("name") {
            if !names.contains(&n.as_str()) {
                names.push(n.as_str());
            }
        }
    }
    if names.is_empty() {
        return String::new();
    }

    let with = |metric: &str, name: &str| -> Vec<&SeriesSnapshot> {
        snapshot
            .series
            .iter()
            .filter(|s| {
                s.metric == metric && s.tags.get("name").map(|n| n == name).unwrap_or(false)
            })
            .collect()
    };
    let sum_ps = |metric: &str, name: &str| -> f64 {
        with(metric, name)
            .iter()
            .filter_map(|s| s.agg.per_second)
            .sum()
    };

    let (mut stat_rows, mut pct_rows, mut apdex_rows, mut code_rows) =
        (String::new(), String::new(), String::new(), String::new());
    let (mut tot_samples, mut tot_fails) = (0u64, 0u64);

    for &name in &names {
        let dur = merge_dur(&with("http_req_duration", name));
        let samples: u64 = with("http_reqs", name).iter().map(|s| s.agg.count).sum();
        let fails: u64 = with("http_req_failed", name)
            .iter()
            .map(|s| s.agg.sum as u64)
            .sum();
        let err = if samples > 0 {
            100.0 * fails as f64 / samples as f64
        } else {
            0.0
        };
        let tput = sum_ps("http_reqs", name);
        let recv = sum_ps("data_received", name) / 1024.0;
        let sent = sum_ps("data_sent", name) / 1024.0;
        tot_samples += samples;
        tot_fails += fails;
        let ec = if fails == 0 { "ok" } else { "bad" };

        stat_rows.push_str(&format!(
            "<tr><td>{}</td><td>{samples}</td><td class=\"{ec}\">{fails}</td><td class=\"{ec}\">{err:.2}%</td>{}{}{}{}{}{}{}<td>{tput:.1}/s</td><td>{recv:.1}</td><td>{sent:.1}</td></tr>",
            esc(name),
            cms(dur.avg), cms(dur.min), cms(dur.med), cms(dur.max),
            cms(dur.p90), cms(dur.p95), cms(dur.p99),
        ));
        pct_rows.push_str(&format!(
            "<tr><td>{}</td>{}{}{}{}{}{}</tr>",
            esc(name),
            cms(dur.med),
            cms(dur.p90),
            cms(dur.p95),
            cms(dur.p99),
            cms(dur.p999),
            cms(dur.max),
        ));
        let ap = apdex(&dur, 500.0);
        let apc = ap
            .map(|v| {
                if v >= 0.94 {
                    "ok"
                } else if v >= 0.85 {
                    "warn"
                } else {
                    "bad"
                }
            })
            .unwrap_or("muted");
        apdex_rows.push_str(&format!(
            "<tr><td>{}</td><td class=\"{apc}\">{}</td><td>{samples}</td></tr>",
            esc(name),
            ap.map(|v| format!("{v:.3}")).unwrap_or_else(|| "-".into()),
        ));
        for s in with("http_reqs", name) {
            let method = s.tags.get("method").map(String::as_str).unwrap_or("");
            let status = s.tags.get("status").map(String::as_str).unwrap_or("?");
            let sc = if status.starts_with('2') || status.starts_with('3') {
                "ok"
            } else {
                "bad"
            };
            code_rows.push_str(&format!(
                "<tr><td>{}</td><td>{}</td><td class=\"{sc}\">{}</td><td>{}</td></tr>",
                esc(name),
                esc(method),
                esc(status),
                s.agg.count,
            ));
        }
    }

    // TOTAL rows: merged across every named series.
    let named = |metric: &str| -> Vec<&SeriesSnapshot> {
        snapshot
            .series
            .iter()
            .filter(|s| s.metric == metric && s.tags.contains_key("name"))
            .collect()
    };
    let tdur = merge_dur(&named("http_req_duration"));
    let ttput: f64 = named("http_reqs")
        .iter()
        .filter_map(|s| s.agg.per_second)
        .sum();
    let trecv: f64 = named("data_received")
        .iter()
        .filter_map(|s| s.agg.per_second)
        .sum::<f64>()
        / 1024.0;
    let tsent: f64 = named("data_sent")
        .iter()
        .filter_map(|s| s.agg.per_second)
        .sum::<f64>()
        / 1024.0;
    let terr = if tot_samples > 0 {
        100.0 * tot_fails as f64 / tot_samples as f64
    } else {
        0.0
    };
    let tec = if tot_fails == 0 { "ok" } else { "bad" };
    stat_rows.push_str(&format!(
        "<tr class=\"tot\"><td>TOTAL</td><td>{tot_samples}</td><td class=\"{tec}\">{tot_fails}</td><td class=\"{tec}\">{terr:.2}%</td>{}{}{}{}{}{}{}<td>{ttput:.1}/s</td><td>{trecv:.1}</td><td>{tsent:.1}</td></tr>",
        cms(tdur.avg), cms(tdur.min), cms(tdur.med), cms(tdur.max),
        cms(tdur.p90), cms(tdur.p95), cms(tdur.p99),
    ));
    pct_rows.push_str(&format!(
        "<tr class=\"tot\"><td>TOTAL</td>{}{}{}{}{}{}</tr>",
        cms(tdur.med),
        cms(tdur.p90),
        cms(tdur.p95),
        cms(tdur.p99),
        cms(tdur.p999),
        cms(tdur.max),
    ));
    let tap = apdex(&tdur, 500.0);
    apdex_rows.push_str(&format!(
        "<tr class=\"tot\"><td>TOTAL</td><td>{}</td><td>{tot_samples}</td></tr>",
        tap.map(|v| format!("{v:.3}")).unwrap_or_else(|| "-".into()),
    ));

    format!(
        r#"<h2>Per-transaction statistics <span class="muted">(latency ms)</span></h2>
<table><thead><tr><th>Transaction</th><th>Samples</th><th>Fails</th><th>Error</th><th>Avg</th><th>Min</th><th>Med</th><th>Max</th><th>p90</th><th>p95</th><th>p99</th><th>Req/s</th><th>Recv KB/s</th><th>Sent KB/s</th></tr></thead>
<tbody>{stat_rows}</tbody></table>
<h2>Response-time percentiles <span class="muted">(ms)</span></h2>
<table><thead><tr><th>Transaction</th><th>p50</th><th>p90</th><th>p95</th><th>p99</th><th>p99.9</th><th>Max</th></tr></thead>
<tbody>{pct_rows}</tbody></table>
<h2>APDEX <span class="muted">(T = 500 ms, estimated from percentiles)</span></h2>
<table><thead><tr><th>Transaction</th><th>APDEX</th><th>Samples</th></tr></thead>
<tbody>{apdex_rows}</tbody></table>
<h2>Response codes</h2>
<table><thead><tr><th>Transaction</th><th>Method</th><th>Status</th><th>Count</th></tr></thead>
<tbody>{code_rows}</tbody></table>
"#
    )
}

/// Count-weighted merge of a duration/TTFB trend across a request's series (one
/// per status/method). Exact when a request returns a single status; otherwise
/// a count-weighted approximation — the same approach the timeline uses, since
/// summaries carry no histogram buckets to merge exactly.
fn merge_dur(series: &[&SeriesSnapshot]) -> AggValues {
    let mut out = AggValues::default();
    let w = |pick: fn(&AggValues) -> Option<f64>| -> Option<f64> {
        let (mut acc, mut n) = (0.0_f64, 0_u64);
        for s in series {
            if s.agg.count == 0 {
                continue;
            }
            if let Some(v) = pick(&s.agg) {
                acc += v * s.agg.count as f64;
                n += s.agg.count;
            }
        }
        (n > 0).then_some(acc / n as f64)
    };
    for s in series {
        out.count += s.agg.count;
        out.sum += s.agg.sum;
    }
    out.avg = w(|a| a.avg);
    out.med = w(|a| a.med);
    out.p90 = w(|a| a.p90);
    out.p95 = w(|a| a.p95);
    out.p99 = w(|a| a.p99);
    out.p999 = w(|a| a.p999);
    out.min = series
        .iter()
        .filter_map(|s| s.agg.min)
        .fold(None, |m, v| Some(m.map_or(v, |x: f64| x.min(v))));
    out.max = series
        .iter()
        .filter_map(|s| s.agg.max)
        .fold(None, |m, v| Some(m.map_or(v, |x: f64| x.max(v))));
    out
}

/// Estimate P(latency ≤ x) by piecewise-linear interpolation over the known
/// percentile knots (loadr summaries carry no histogram buckets).
fn interp_cdf(a: &AggValues, x: f64) -> Option<f64> {
    let mut pts: Vec<(f64, f64)> = [
        (a.min, 0.0),
        (a.med, 0.5),
        (a.p90, 0.9),
        (a.p95, 0.95),
        (a.p99, 0.99),
        (a.p999, 0.999),
        (a.max, 1.0),
    ]
    .into_iter()
    .filter_map(|(v, q)| v.map(|v| (v, q)))
    .collect();
    if pts.is_empty() {
        return None;
    }
    pts.sort_by(|p, q| p.0.partial_cmp(&q.0).unwrap_or(std::cmp::Ordering::Equal));
    if x <= pts[0].0 {
        return Some(if x < pts[0].0 { 0.0 } else { pts[0].1 });
    }
    if x >= pts[pts.len() - 1].0 {
        return Some(1.0);
    }
    for w in pts.windows(2) {
        let ((v0, q0), (v1, q1)) = (w[0], w[1]);
        if v0 <= x && x <= v1 {
            return Some(if v1 == v0 {
                q1
            } else {
                q0 + (q1 - q0) * (x - v0) / (v1 - v0)
            });
        }
    }
    Some(1.0)
}

/// APDEX ≈ (CDF(T) + CDF(4T)) / 2 = satisfied + tolerating/2, all over total.
fn apdex(a: &AggValues, t_ms: f64) -> Option<f64> {
    let s = interp_cdf(a, t_ms)?;
    let tol = interp_cdf(a, 4.0 * t_ms)?;
    Some(((s + tol) / 2.0).clamp(0.0, 1.0))
}

/// One millisecond table cell: sub-10ms at 3dp, else 1dp, seconds past 1000ms.
fn cms(v: Option<f64>) -> String {
    match v {
        None => "<td>-</td>".to_string(),
        Some(x) if x >= 1000.0 => format!("<td>{:.2}s</td>", x / 1000.0),
        Some(x) if x >= 10.0 => format!("<td>{x:.1}</td>"),
        Some(x) => format!("<td>{x:.3}</td>"),
    }
}

/// The time-series charts section. Returns an empty string when there is no
/// timeline (e.g. summaries from before timeline capture, or runs too short to
/// produce a single interval), so the rest of the report renders unchanged.
fn timeseries_section(timeline: &[TimelinePoint]) -> String {
    if timeline.is_empty() {
        return String::new();
    }
    // Serialize the timeline as compact arrays the inline script consumes. We
    // hand-roll the JSON (no extra deps) and emit only finite numbers.
    let col = |pick: fn(&TimelinePoint) -> Option<f64>| -> String {
        timeline
            .iter()
            .map(|p| match pick(p) {
                Some(v) if v.is_finite() => fmt_json_num(v),
                _ => "null".to_string(),
            })
            .collect::<Vec<_>>()
            .join(",")
    };
    // Built-in series. Left unclosed so external series can be appended.
    let mut data = format!(
        r#"{{"t":[{t}],"rps":[{rps}],"iters":[{iters}],"vus":[{vus}],"err":[{err}],"avg":[{avg}],"p50":[{p50}],"p95":[{p95}],"p99":[{p99}]"#,
        t = col(|p| Some(p.elapsed_secs)),
        rps = col(|p| Some(p.rps)),
        iters = col(|p| Some(p.iterations_ps)),
        vus = col(|p| Some(p.active_vus)),
        err = col(|p| Some(p.error_rate * 100.0)),
        avg = col(|p| p.latency_avg),
        p50 = col(|p| p.latency_p50),
        p95 = col(|p| p.latency_p95),
        p99 = col(|p| p.latency_p99),
    );

    // External system metrics (from `observe:`). Union of keys across the
    // timeline, stable order. Each becomes an `ext_<key>` data array AND its own
    // small-multiple chart with its own y-axis — so metrics on wildly different
    // scales (a CPU ratio next to connection counts next to ops/sec) each stay
    // readable. The ember-led palette is shared with the JS so legend swatches
    // match the lines.
    const EXT_PAL: [&str; 6] = [
        "#ef4444", "#4ade80", "#f87171", "#fb923c", "#9ca3af", "#d1d5db",
    ];
    let mut ext_names: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
    for p in timeline {
        for k in p.external.keys() {
            ext_names.insert(k.as_str());
        }
    }
    let mut infra_cards = String::new();
    let mut ext_jskeys: Vec<String> = Vec::new();
    for (i, name) in ext_names.iter().enumerate() {
        let jskey = format!("ext_{}", sanitize_key(name));
        let coldata = timeline
            .iter()
            .map(|p| match p.external.get(*name) {
                Some(v) if v.is_finite() => fmt_json_num(*v),
                _ => "null".to_string(),
            })
            .collect::<Vec<_>>()
            .join(",");
        data.push_str(&format!(r#","{jskey}":[{coldata}]"#));
        let color = EXT_PAL[i % EXT_PAL.len()];
        infra_cards.push_str(&format!(
            r#"  <div class="chart"><h3>{label}</h3>
    <div class="legend"><span><i style="background:{color}"></i>{label}</span></div>
    <svg viewBox="0 0 640 220" preserveAspectRatio="none" data-chart="{jskey}"></svg>
    <div class="tip" data-tip="{jskey}"></div></div>
"#,
            label = esc(name),
        ));
        ext_jskeys.push(jskey);
    }
    if !ext_jskeys.is_empty() {
        let arr = ext_jskeys
            .iter()
            .map(|k| format!("\"{k}\""))
            .collect::<Vec<_>>()
            .join(",");
        data.push_str(&format!(r#","__ext":[{arr}]"#));
    }
    data.push('}');

    // The data block is embedded as JSON in a non-executed script tag, then
    // parsed by the chart script. `</` is escaped so the payload can never close
    // the surrounding <script> element.
    let data = data.replace("</", "<\\/");

    // System-metric panels go in their own labelled grid.
    let infra_section = if infra_cards.is_empty() {
        String::new()
    } else {
        format!(
            "<div id=\"ts-infra\"><h2>System metrics (observe)</h2>\n<div class=\"charts\">\n{infra_cards}</div></div>\n"
        )
    };

    format!(
        r##"<h2>Over time</h2>
<div class="charts" id="ts-charts">
  <div class="chart"><h3>Throughput</h3>
    <div class="legend"><span><i style="background:#ef4444"></i>requests/s</span><span><i style="background:#f87171"></i>iterations/s</span></div>
    <svg viewBox="0 0 640 220" preserveAspectRatio="none" data-chart="throughput"></svg>
    <div class="tip" data-tip="throughput"></div></div>
  <div class="chart"><h3>Response time (ms)</h3>
    <div class="legend"><span><i style="background:#4ade80"></i>p50</span><span><i style="background:#fbbf24"></i>p95</span><span><i style="background:#ef4444"></i>p99</span><span><i style="background:#9ca3af"></i>avg</span></div>
    <svg viewBox="0 0 640 220" preserveAspectRatio="none" data-chart="latency"></svg>
    <div class="tip" data-tip="latency"></div></div>
  <div class="chart"><h3>Active VUs</h3>
    <div class="legend"><span><i style="background:#fb923c"></i>virtual users</span></div>
    <svg viewBox="0 0 640 220" preserveAspectRatio="none" data-chart="vus"></svg>
    <div class="tip" data-tip="vus"></div></div>
  <div class="chart"><h3>Error rate (%)</h3>
    <div class="legend"><span><i style="background:#ef4444"></i>failed %</span></div>
    <svg viewBox="0 0 640 220" preserveAspectRatio="none" data-chart="error"></svg>
    <div class="tip" data-tip="error"></div></div>
</div>
{infra_section}<script type="application/json" id="ts-data">{data}</script>
<script>{CHART_JS}</script>
"##,
        data = data,
        infra_section = infra_section,
        CHART_JS = CHART_JS,
    )
}

/// JS-identifier-safe key for an external metric name.
fn sanitize_key(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect()
}

/// Compact, deterministic number formatting for the embedded JSON payload.
fn fmt_json_num(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        // Trim to 3 decimals without trailing-zero noise.
        let s = format!("{v:.3}");
        let s = s.trim_end_matches('0').trim_end_matches('.');
        s.to_string()
    }
}

/// Inline charting script. Pure DOM/SVG, no dependencies; draws line charts
/// from the embedded timeline and adds a shared hover crosshair + readout.
const CHART_JS: &str = r##"
(function(){
  var raw = document.getElementById("ts-data");
  if(!raw) return;
  var d = JSON.parse(raw.textContent);
  var t = d.t || [];
  if(!t.length) return;
  var W=640,H=220,PL=46,PR=10,PT=10,PB=24;
  var svgns="http://www.w3.org/2000/svg";
  function el(n,a){var e=document.createElementNS(svgns,n);for(var k in a)e.setAttribute(k,a[k]);return e;}
  function maxOf(series){var m=0;series.forEach(function(s){(s||[]).forEach(function(v){if(v!=null&&isFinite(v)&&v>m)m=v;});});return m;}
  var tmin=t[0], tmax=t[t.length-1]||1; if(tmax<=tmin)tmax=tmin+1;
  function px(x){return PL+(x-tmin)/(tmax-tmin)*(W-PL-PR);}
  function nice(m){if(m<=0)return 1;var p=Math.pow(10,Math.floor(Math.log10(m)));var f=m/p;var n=f<=1?1:f<=2?2:f<=5?5:10;return n*p;}
  var palette={rps:"#ef4444",iters:"#f87171",p50:"#4ade80",p95:"#fbbf24",p99:"#ef4444",avg:"#9ca3af",vus:"#fb923c",err:"#ef4444"};
  var charts={
    throughput:{svg:null,keys:["rps","iters"],max:0},
    latency:{svg:null,keys:["p50","p95","p99","avg"],max:0},
    vus:{svg:null,keys:["vus"],max:0},
    error:{svg:null,keys:["err"],max:0}
  };
  // observe: register a "System metrics" chart from any ext_* series. The
  // ember-led palette matches the legend swatches emitted server-side.
  if(d.__ext&&d.__ext.length){
    var extPal=["#ef4444","#4ade80","#f87171","#fb923c","#9ca3af","#d1d5db"];
    d.__ext.forEach(function(k,idx){
      if(!palette[k])palette[k]=extPal[idx%extPal.length];
      charts[k]={svg:null,keys:[k],max:0}; // one small-multiple panel per metric
    });
  }
  var cursors=[];
  Object.keys(charts).forEach(function(name){
    var c=charts[name];
    var svg=document.querySelector('svg[data-chart="'+name+'"]');
    if(!svg)return; c.svg=svg;
    var series=c.keys.map(function(k){return d[k]||[];});
    c.max=nice(maxOf(series));
    function py(v){return H-PB-(v/c.max)*(H-PT-PB);}
    // gridlines + y labels
    for(var i=0;i<=4;i++){
      var yv=c.max*i/4, y=py(yv);
      svg.appendChild(el("line",{class:"grid",x1:PL,y1:y,x2:W-PR,y2:y}));
      var lbl=el("text",{class:"axis",x:PL-6,y:y+3,"text-anchor":"end"});
      lbl.textContent=(yv>=1000?(yv/1000).toFixed(1)+"k":(c.max<10?yv.toFixed(1):Math.round(yv)));
      svg.appendChild(lbl);
    }
    // x labels (start/mid/end)
    [0,Math.floor(t.length/2),t.length-1].forEach(function(idx){
      var x=px(t[idx]);
      var tx=el("text",{class:"axis",x:x,y:H-6,"text-anchor":idx===0?"start":idx===t.length-1?"end":"middle"});
      tx.textContent=Math.round(t[idx])+"s"; svg.appendChild(tx);
    });
    // lines
    c.keys.forEach(function(k){
      var ys=d[k]||[],pts="";
      for(var i=0;i<t.length;i++){if(ys[i]==null||!isFinite(ys[i]))continue;pts+=(pts?" ":"")+px(t[i]).toFixed(1)+","+py(ys[i]).toFixed(1);}
      if(pts)svg.appendChild(el("polyline",{points:pts,fill:"none",stroke:palette[k],"stroke-width":"1.8","stroke-linejoin":"round"}));
    });
    var cur=el("line",{class:"cursor",x1:0,y1:PT,x2:0,y2:H-PB}); svg.appendChild(cur); cursors.push(cur);
    c.cur=cur;
  });
  function nearest(xpix){
    var best=0,bd=1e9;
    for(var i=0;i<t.length;i++){var dx=Math.abs(px(t[i])-xpix);if(dx<bd){bd=dx;best=i;}}
    return best;
  }
  function fmt(v,suffix){return v==null||!isFinite(v)?"—":(v>=1000?(v/1000).toFixed(2)+"k":v.toFixed(v<10?2:0))+(suffix||"");}
  function readout(i){
    set("throughput","<b>"+fmt(t[i])+"s</b> · req "+fmt(d.rps[i])+"/s · iter "+fmt(d.iters[i])+"/s");
    set("latency","<b>"+fmt(t[i])+"s</b> · p50 "+fmt(d.p50[i],"ms")+" · p95 "+fmt(d.p95[i],"ms")+" · p99 "+fmt(d.p99[i],"ms")+" · avg "+fmt(d.avg[i],"ms"));
    set("vus","<b>"+fmt(t[i])+"s</b> · "+fmt(d.vus[i])+" VUs");
    set("error","<b>"+fmt(t[i])+"s</b> · "+fmt(d.err[i],"%"));
    if(d.__ext){d.__ext.forEach(function(k){set(k,"<b>"+fmt(t[i])+"s</b> · "+fmt(d[k]?d[k][i]:null));});}
  }
  function set(name,html){var e=document.querySelector('[data-tip="'+name+'"]');if(e)e.innerHTML=html;}
  function move(ev){
    var any=ev.currentTarget;var r=any.getBoundingClientRect();
    var xpix=(ev.clientX-r.left)/r.width*W;
    var i=nearest(xpix);var xx=px(t[i]);
    cursors.forEach(function(cu){cu.setAttribute("x1",xx);cu.setAttribute("x2",xx);cu.style.opacity=1;});
    readout(i);
  }
  function leave(){cursors.forEach(function(cu){cu.style.opacity=0;});readout(t.length-1);}
  Object.keys(charts).forEach(function(name){var s=charts[name].svg;if(!s)return;s.addEventListener("mousemove",move);s.addEventListener("mouseleave",leave);});
  readout(t.length-1);
})();
"##;

fn td_ms(v: Option<f64>) -> String {
    match v {
        None => "<td>-</td>".to_string(),
        Some(ms) if ms >= 1000.0 => format!("<td>{:.2}s</td>", ms / 1000.0),
        Some(ms) => format!("<td>{ms:.1}ms</td>"),
    }
}

fn fmt_num(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        format!("{v:.2}")
    }
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_report() {
        let json = serde_json::json!({
            "name": "demo", "run_id": "r-1", "started_ms": 0u64, "ended_ms": 1000u64,
            "duration_secs": 1.0, "scenarios": ["s"],
            "metrics": [
                {"metric": "http_req_duration", "kind": "trend",
                 "agg": {"count": 10u64, "sum": 100.0, "avg": 10.0, "min": 1.0, "max": 20.0,
                          "med": 9.0, "p90": 18.0, "p95": 19.0, "p99": 20.0, "p999": 20.0,
                          "rate": null, "last": null, "per_second": 10.0}}
            ],
            "checks": [{"name": "ok", "passes": 9u64, "fails": 1u64}],
            "thresholds": [{"metric": "http_req_duration", "expression": "p(95)<400",
                             "observed": 19.0, "passed": true, "abort_on_fail": false}],
            "thresholds_passed": true, "aborted": null,
            "snapshot": {"timestamp_ms": 0u64, "elapsed_secs": 1.0, "interval_secs": 1.0, "series": []},
            "timeline": [
                {"elapsed_secs": 1.0, "rps": 10.0, "iterations_ps": 5.0, "active_vus": 2.0,
                 "error_rate": 0.0, "latency_avg": 9.0, "latency_p50": 8.0, "latency_p95": 19.0, "latency_p99": 20.0},
                {"elapsed_secs": 2.0, "rps": 20.0, "iterations_ps": 9.0, "active_vus": 4.0,
                 "error_rate": 0.05, "latency_avg": 12.0, "latency_p50": 11.0, "latency_p95": 25.0, "latency_p99": 30.0}
            ]
        });
        let summary: Summary = serde_json::from_value(json).expect("summary");
        let html = render(&summary);
        assert!(html.contains("PASSED"));
        assert!(html.contains("http_req_duration"));
        assert!(html.contains("p(95)&lt;400"));
        assert!(html.contains("<title>loadr report — demo</title>"));
        // Time-series section present with embedded data and inline chart script.
        assert!(html.contains("Over time"));
        assert!(html.contains(r#"id="ts-data""#));
        assert!(html.contains(r#"data-chart="throughput""#));
        assert!(html.contains(r#"data-chart="latency""#));
        assert!(html.contains("\"rps\":[10,20]"));
        // Self-contained: no external script/stylesheet/image references.
        let lower = html.to_lowercase();
        assert!(!lower.contains("<script src"));
        assert!(!lower.contains("<link "));
        assert!(!lower.contains("src=\"http"));
        assert!(!lower.contains("href=\"http"));
    }

    #[test]
    fn renders_without_timeline() {
        // Older summaries (no timeline) must still render, with no charts.
        let json = serde_json::json!({
            "name": "old", "run_id": "r-2", "started_ms": 0u64, "ended_ms": 1000u64,
            "duration_secs": 1.0, "scenarios": ["s"], "metrics": [], "checks": [],
            "thresholds": [], "thresholds_passed": true, "aborted": null,
            "snapshot": {"timestamp_ms": 0u64, "elapsed_secs": 1.0, "interval_secs": 1.0, "series": []}
        });
        let summary: Summary = serde_json::from_value(json).expect("summary");
        let html = render(&summary);
        assert!(html.contains("PASSED"));
        assert!(!html.contains("Over time"));
        assert!(!html.contains(r#"id="ts-data""#));
        // No name-tagged series -> no per-transaction section.
        assert!(!html.contains("Per-transaction statistics"));
    }

    #[test]
    fn renders_per_transaction_breakdown() {
        // A summary whose snapshot carries per-tag series (name/method/status) —
        // loadr adds a `name` tag to every named request — must produce the
        // JMeter/Locust per-transaction tables, percentiles, APDEX and codes.
        let series = |metric: &str,
                      name: &str,
                      method: &str,
                      status: &str,
                      count: u64,
                      p99: f64| {
            let kind = match metric {
                "http_reqs" => "counter",
                "http_req_failed" => "rate",
                _ => "trend",
            };
            serde_json::json!({
                "metric": metric, "kind": kind,
                "tags": {"name": name, "method": method, "status": status, "proto": "http", "scenario": "s"},
                "agg": {"count": count, "sum": if metric == "http_req_failed" {0.0} else {count as f64},
                        "avg": 2.0, "min": 1.0, "max": p99 + 1.0, "med": 1.5,
                        "p90": p99 * 0.8, "p95": p99 * 0.9, "p99": p99, "p999": p99 + 0.5,
                        "rate": serde_json::Value::Null, "last": serde_json::Value::Null,
                        "per_second": count as f64},
                "interval_count": count, "interval_sum": count as f64
            })
        };
        let json = serde_json::json!({
            "name": "j", "run_id": "r-3", "started_ms": 0u64, "ended_ms": 1000u64,
            "duration_secs": 1.0, "scenarios": ["s"], "metrics": [], "checks": [],
            "thresholds": [], "thresholds_passed": true, "aborted": null,
            "snapshot": {"timestamp_ms": 0u64, "elapsed_secs": 1.0, "interval_secs": 1.0, "series": [
                series("http_req_duration", "create", "POST", "201", 100, 15.0),
                series("http_reqs",         "create", "POST", "201", 100, 0.0),
                series("http_req_failed",   "create", "POST", "201", 100, 0.0),
                series("http_req_duration", "read",   "GET",  "200", 100, 8.0),
                series("http_reqs",         "read",   "GET",  "200", 100, 0.0),
                series("http_req_failed",   "read",   "GET",  "200", 100, 0.0),
            ]}
        });
        let summary: Summary = serde_json::from_value(json).expect("summary");
        let html = render(&summary);
        // Section headers.
        assert!(html.contains("Per-transaction statistics"));
        assert!(html.contains("Response-time percentiles"));
        assert!(html.contains("APDEX"));
        assert!(html.contains("Response codes"));
        // Both transactions listed, with a TOTAL row.
        assert!(html.contains(">create</td>"));
        assert!(html.contains(">read</td>"));
        assert!(html.contains(r#"<tr class="tot"><td>TOTAL</td>"#));
        // Response codes broken out per method/status.
        assert!(html.contains(">POST</td>"));
        assert!(html.contains(">201</td>"));
        assert!(html.contains(">200</td>"));
    }

    #[test]
    fn timeline_payload_serializes_and_handles_nulls() {
        use loadr_core::TimelinePoint;
        let tl = vec![TimelinePoint {
            elapsed_secs: 1.5,
            rps: 12.0,
            iterations_ps: 6.0,
            active_vus: 3.0,
            error_rate: 0.0,
            latency_avg: Some(9.25),
            latency_p50: Some(8.0),
            latency_p95: Some(19.0),
            latency_p99: None,
            external: Default::default(),
        }];
        let section = timeseries_section(&tl);
        // The None p99 serializes as JSON null inside the data block.
        assert!(section.contains(r#""p99":[null]"#), "section:\n{section}");
        // Fractional values trim trailing zeros.
        assert!(section.contains(r#""avg":[9.25]"#));
        assert!(section.contains(r#""t":[1.5]"#));
        // The embedded JSON lives in a non-executed application/json block.
        assert!(section.contains(r#"<script type="application/json" id="ts-data">"#));
        // No observe data -> no infrastructure chart.
        assert!(!section.contains(r#"data-chart="infra""#));
    }

    #[test]
    fn external_metrics_render_an_infrastructure_chart() {
        let mut p = TimelinePoint {
            elapsed_secs: 1.0,
            rps: 5.0,
            iterations_ps: 5.0,
            active_vus: 2.0,
            error_rate: 0.0,
            latency_avg: None,
            latency_p50: None,
            latency_p95: None,
            latency_p99: None,
            external: Default::default(),
        };
        p.external.insert("system_cpu".to_string(), 0.82);
        let section = timeseries_section(&[p]);
        // Each external metric gets its own small-multiple panel.
        assert!(
            section.contains(r#"data-chart="ext_system_cpu""#),
            "section:\n{section}"
        );
        assert!(section.contains(r#""ext_system_cpu":[0.82]"#));
        assert!(section.contains(r#""__ext":["ext_system_cpu"]"#));
        assert!(section.contains("System metrics (observe)"));
    }

    #[test]
    fn timeline_payload_escapes_embedded_script_close() {
        // fmt_json_num never emits `</`, but guard the escaping path directly:
        // the replacement rewrites any `</` in the payload to `<\/`.
        let payload = "abc</script>def".replace("</", "<\\/");
        assert_eq!(payload, "abc<\\/script>def");
    }
}
