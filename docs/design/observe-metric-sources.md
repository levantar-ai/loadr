# Design RFC — `observe`: system-metric collectors & load↔system correlation

**Status:** draft / for review
**Goal:** let a loadr run pull in **system / infrastructure metrics** (CPU, memory,
disk & network I/O, DB connections, GC, queue depth, …) from whatever the user
already runs — Prometheus, OpenTelemetry, CloudWatch, StatsD, a plain JSON
endpoint — **normalize them into loadr's existing metric model**, and present
them on the **same timeline** as the load metrics. One pane, load-vs-system,
with correlation, overlays, and thresholds that can gate on the target's health.

This is the inverse of [`outputs`](../src/yaml/outputs.md) (which streams loadr's
metrics *out*); `observe` pulls foreign metrics *in*.

---

## 1. Motivation

A load test answers *"how fast / how much"*. It does **not**, on its own, answer
*"why did it stop scaling"*. When throughput plateaus and p99 climbs, the cause —
CPU saturation, a maxed connection pool, GC pauses, disk contention, a noisy
neighbour — lives in **system metrics on the target**, which today sit in a
*different* tool (Grafana, CloudWatch). The human has to eyeball two screens and
mentally align timestamps.

The high-value, currently-unsolved problem is **correlation**, not collection:

- **k6 (OSS)** punts entirely to Grafana.
- **Gatling** gates system correlation behind Gatling Enterprise.
- **JMeter** ships the *PerfMon ServerAgent* — a bespoke agent you install on
  every target, on an old protocol, lightly maintained. We explicitly do **not**
  want to repeat that (see Non-goals).

loadr is uniquely placed to do this well because it already:

- records into a normalized metric model (`Counter` / `Rate` / `Gauge` /
  `Trend`) with tag sets;
- merges per-second snapshots **centrally** on the controller (see
  [metric aggregation](../src/distributed/metrics-merging.md));
- renders a time-series HTML report and a live UI from that timeline;
- evaluates thresholds centrally with tag selectors;
- re-exports everything (Prometheus / OTLP / InfluxDB) via `outputs`.

If foreign metrics are normalized into that same model, **every one of those
capabilities applies to them for free**.

### Worked example

The `loadr-demos` stress test plateaus at ~2.2k req/s while VUs ramp to 80 and
p99 climbs 0→170ms. Is that CPU or the DB pool? With `observe`, the report would
overlay `system_cpu{instance:api-1}` and `pg_pool_in_use{instance:api-1}` on the
same x-axis as `http_req_duration:p99` — and the answer is one glance, not a
two-tab investigation. A threshold (`system_cpu: value<0.9`) could even abort the
run the moment the box redlines.

---

## 2. Non-goals

- **Not a standing APM / monitoring product.** `observe` collects only for the
  run's time window and only to correlate with that run. No alerting, no
  long-term storage, no dashboards beyond the run report.
- **Not a collection agent on targets.** We do **not** ship a process that runs
  on the system under test to read `/proc`. That re-invents
  node_exporter/Telegraf/OTel poorly and adds a privileged process + open port
  to every target. Collection is **pull**, from the controller, against metrics
  backends the user *already* operates. For users without one, we ship a
  compose/Helm *recipe* wiring node_exporter/Telegraf → loadr, not a new agent.
- **Not push.** Sources are queried by loadr; nothing inbound to the load
  generators. (An OTLP *receiver* mode is a possible later exception — see Open
  questions.)

---

## 3. Overview

```text
 plan.yaml ── observe: ──▶  ┌─────────────── controller ───────────────┐
   - type: prometheus       │  collectors (one per source)             │
   - type: cloudwatch       │    PromQL │ OTLP │ CloudWatch │ StatsD …  │
   - type: http-json        │        │ each emits ExternalSample        │
                            │        ▼                                  │
                            │  normalize → loadr metric model           │
                            │  (Counter/Rate/Gauge/Trend, tagged)       │
                            │        │                                  │
                            │        ▼  merged onto the run timeline     │
                            │  report overlay · thresholds · re-export  │
                            └───────────────────────────────────────────┘
```

Collection runs **on the controller** (in a distributed run) or in-process (a
single-node run) — the same place load metrics are aggregated, so there is a
single source of truth and the run's authoritative start/stop window is known.

---

## 4. The canonical form

**Decision: the canonical form *is* loadr's existing metric model.** We do not
invent a parallel representation. Each collector maps its backend's response to:

```rust
struct ExternalSample {
    metric: String,         // canonical name, e.g. "system_cpu"
    kind:   MetricKind,     // Gauge | Counter | Rate  (Trend reserved for histograms)
    unit:   Unit,           // Ratio | Percent | Bytes | Count | Seconds | …
    tags:   TagSet,         // { source:"prometheus", instance:"api-1", + user labels }
    ts_ms:  i64,            // resampled onto the run's snapshot grid
    value:  f64,
}
```

Once an `ExternalSample` is ingested it is **indistinguishable from a
loadr-native sample** downstream. Therefore, with zero extra work per consumer:

- it appears as a **timeline series** (report overlay + live UI);
- it is **threshold-able** via the existing engine and tag selectors;
- it is **re-exported** by `outputs` (Prometheus/OTLP/Influx) like any metric;
- in distributed runs it merges centrally like any metric.

Mapping rules:

| Backend concept | loadr kind | Aggregation it supports |
|---|---|---|
| gauge (CPU %, mem bytes, pool size) | `Gauge` | `value`, min/max envelope |
| monotonic counter (bytes_total) | `Counter` | `count`, derived `rate` |
| pre-rated series (req/s already) | `Rate` | `rate` |
| (future) server-side histogram | `Trend` | `p(n)` via HDR import |

`unit` drives axis formatting (bytes→KiB/MiB, ratio→%) and sane default
threshold bounds; it never changes the stored value.

**Naming convention.** Canonical names are snake_case with a domain prefix:
`system_cpu`, `system_mem_used_bytes`, `db_connections_in_use`,
`runtime_gc_pause_seconds`. Collectors may rename freely (`as:` field); the raw
backend name is preserved in a `raw` tag.

---

## 5. The collector seam

Mirrors the existing protocol registry and `outputs` factory.

```rust
#[async_trait]
trait MetricSource: Send + Sync {
    /// Collect samples for [window.start, window.end] at `step` resolution,
    /// already normalized to ExternalSample. Called once at run end, and
    /// incrementally each snapshot interval for the live UI.
    async fn collect(&self, window: TimeWindow, step: Duration)
        -> Result<Vec<ExternalSample>, SourceError>;

    fn name(&self) -> &str;
}
```

Built-in implementations (phased — see §9):

- `PrometheusSource` — HTTP range query (`/api/v1/query_range`) per PromQL expr.
- `OtlpSource` — pull from an OTel collector / Prometheus-compatible endpoint.
- `CloudWatchSource` — `GetMetricData` over the window.
- `StatsdSource` — read from a StatsD/Telegraf aggregator.
- `HttpJsonSource` — poll a URL, extract via JSONPath (the universal escape hatch).
- `plugin` — delegate to a native/WASM plugin (mirrors `outputs`' `type: plugin`),
  so third parties can add sources without a core change.

Registration is identical in spirit to `loadr_protocols::builtin_registry`.

---

## 6. Configuration — `observe:` (type-discriminated)

A new top-level block. Each entry is a serde-tagged enum on `type`; per-source
fields differ. Symmetric with `outputs:`.

```yaml
observe:
  # Prometheus / PromQL (phase 1)
  - name: api cpu
    type: prometheus
    source: http://prometheus:9090
    query: 'avg(rate(node_cpu_seconds_total{instance="api-1",mode!="idle"}[15s]))'
    as: system_cpu          # canonical name (optional)
    unit: ratio
    tags: { instance: api-1 }

  - name: db pool in use
    type: prometheus
    source: http://prometheus:9090
    query: 'pg_stat_activity_count{datname="storefront"}'
    as: db_connections_in_use
    unit: count

  # CloudWatch (later phase)
  - name: rds cpu
    type: cloudwatch
    region: eu-west-2
    namespace: AWS/RDS
    metric: CPUUtilization
    dimensions: { DBInstanceIdentifier: storefront }
    period: 10s
    unit: percent

  # Universal fallback: poll any JSON endpoint
  - name: app heap
    type: http-json
    url: http://api-1:9100/debug/vars
    jsonpath: $.memstats.HeapInuse
    as: runtime_heap_bytes
    unit: bytes
    interval: 5s
```

CLI ad-hoc (mirrors `--output`):

```bash
loadr run --observe 'prometheus=http://prom:9090;query=up' test.yaml
```

Credentials come from the environment / secrets, never inlined:
`token: ${env.PROM_TOKEN}`, AWS via the standard credential chain.

---

## 7. Integration points

- **Timeline / report.** External series are added to the run timeline keyed by
  `(metric, tags)`. The HTML report grows an **"Infrastructure"** chart group
  beside the existing Throughput / Response-time / VUs / Error-rate charts, with
  the option to **overlay** a system series on a load chart (e.g. p99 + CPU on a
  dual axis) so the correlation is literally one chart.
- **Thresholds.** No new syntax — gauges already support the `value` aggregation
  and tag selectors:
  ```yaml
  thresholds:
    "system_cpu{instance:api-1}":           [ "value<0.90" ]   # don't redline
    "db_connections_in_use{instance:api-1}":[ "max<95" ]
  ```
  `abort_on_fail` then stops the run fleet-wide when the *target* is unhealthy,
  not just when the client-side SLO breaks.
- **Outputs / re-export.** External samples flow through `outputs` unchanged, so
  a correlated load+system stream can be shipped to Influx/OTLP in one feed.
- **Distributed.** Collection happens **once, on the controller** — not per
  agent — avoiding N× duplicate scrapes and giving a single coherent system view
  aligned to the merged load timeline.

---

## 8. Cross-cutting concerns

- **Clock alignment.** loadr owns the authoritative `[run_start, run_end]` window
  and snapshot grid; every collector **resamples** its source onto that grid
  (last-value / linear per `kind`), so skew between loadr and the metrics backend
  is corrected. Backends are queried with `step = snapshot interval`.
- **Failure isolation.** A source being unreachable, slow, or returning garbage
  **must never fail the load test**. Errors are logged, the affected series shows
  a gap, and the run summary notes "observe source X: N/M scrapes failed". A
  per-source `required: true` can opt into hard-failing if desired.
- **Security.** Pull-only from the controller; nothing inbound to targets. Creds
  via env/secrets and redacted from logs/exports. PromQL/JSONPath are treated as
  user-trusted config (same trust level as the plan itself).
- **Cost / rate.** Per-source `interval` (default = snapshot interval) bounds
  query volume; CloudWatch `GetMetricData` batching respected to control spend.

---

## 9. Phasing

1. **Canonical model + `MetricSource` seam + `PrometheusSource`** — covers the
   majority of users for the least code; everything downstream (overlay,
   thresholds, export) reuses existing machinery. Vertical slice demoed against
   the `loadr-demos` stack (real load-vs-CPU/DB overlay).
2. **`http-json`** (universal fallback) and **`otlp`**.
3. **`cloudwatch`**, **`statsd`**; **threshold auto-annotation** ("p99 knee at
   t=42s ↔ CPU hit 100%").
4. **`plugin`** source type for community backends; optional **OTLP receiver**
   (push) mode.

---

## 10. Documentation plan

`observe` warrants its own docs section (it sits beside `distributed/`):

```
docs/src/observe/
  overview.md          the load↔system correlation story + quick start
  canonical-model.md   the normalized sample model; kind/unit mapping table
  sources/
    prometheus.md  otlp.md  cloudwatch.md  statsd.md  http-json.md
  thresholds.md        gating a run on target-system health
  writing-a-source.md  implementing MetricSource (the seam) / plugin sources
```

These pages are authored **with** the implementation (not before), so the
published book never documents unbuilt behaviour. `migration/from-jmeter.md`
gains a "PerfMon → observe" subsection.

---

## 11. Open questions

- **OTLP push receiver** vs pull-only — worth the inbound surface? (Lean: pull
  first, receiver later behind a flag.)
- **Auto-discovery** of `instance` tags (match agent/target labels to source
  series automatically) — nice, but is it too magic for v1?
- **Dual-axis overlay UX** — which load series pairs with which system series by
  default, and how much the user configures vs. loadr infers.
- **Threshold annotation** — detecting and labelling the correlation point
  (knee) automatically is high-wow but needs a heuristic we trust.
- Naming: `observe:` vs `monitors:` vs `correlate:` for the block.
