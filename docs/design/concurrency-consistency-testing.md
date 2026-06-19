# Design RFC — concurrency & consistency testing ("loadr chaos")

**Status:** draft / for review
**Goal:** let loadr stand up (or attach to) a clustered system, drive a
concurrent workload, inject faults (network partitions, node loss, latency,
clock skew), and **check the recorded history for consistency violations** —
i.e. empirically classify a system's CAP behaviour and catch real concurrency
bugs (lost updates, stale/dirty reads, broken invariants).

This is loadr's take on what **Jepsen** does, but integrated into the existing
load-test binary, plan format and protocol-plugin ecosystem rather than a
separate Clojure toolchain.

---

## 1. Motivation

Load testing answers "how fast / how much". It does **not** answer "is the
system still *correct* when it's under concurrent load *and* something breaks".
The CAP theorem says that under a network **P**artition a system must trade
**C**onsistency against **A**vailability — but which way a given config
actually behaves (MongoDB `w:majority` vs `w:1`, Postgres sync vs async
replicas, Redis Sentinel failover, Kafka `acks=all`) is an empirical question
most teams never test. The failure modes — a read that returns stale data after
a newer write was acknowledged, a counter that loses increments during a
failover, a bank balance that goes negative under contention — only appear
when **concurrency + faults** happen together.

loadr already provides the concurrency engine, the protocol drivers, and a
distributed control plane. The missing pieces are **fault injection**, a
**structured operation history**, and a **consistency checker**.

### Prior art (and what we borrow)
- **Jepsen** — randomized op generation + "nemesis" faults + history checking.
- **Elle** — polynomial-time transactional-anomaly checker via dependency-graph
  cycle detection (G0/G1/G2). Practical for SQL/transactions.
- **Knossos / Porcupine / Wing–Gong** — linearizability checkers for
  single-object (register/kv) histories.
- **toxiproxy / Pumba / chaos-mesh / `tc`/`iptables`** — mechanisms for
  partitions, latency, packet loss, container kills.

We reuse the *models* (op semantics, real-time history, nemesis schedule,
cycle-detection checking) and implement a pragmatic subset natively in Rust,
with escape hatches to external checkers.

---

## 2. What loadr already has (and reuses)

| Capability | Reused for |
|---|---|
| Scenarios + executors (constant-vus, arrival-rate, ramping) | concurrency level / contention |
| `rendezvous` barrier step | synchronized bursts to maximise contention windows |
| Protocol plugins (mongo, postgres, mysql, redis, kafka, …) | the operations the workload issues |
| Extractors + `${vars}` | read-your-writes style flows |
| Sample/metric stream with timestamps + `extras` | basis for the operation history |
| Distributed agents + HDR-merged percentiles | scale + per-region availability measurement |
| HTML time-series report | overlaying faults vs availability vs anomalies |
| docker-compose harness | bringing clusters up for examples/CI |
| QuickJS embedding | custom op generators / custom checks |

The design is deliberately **additive**: three new plan blocks, one new output,
one new driver kind, one new crate.

---

## 3. Design overview

The consistency-testing pipeline, end to end:

```
            ┌──────────────────────────────────────────────────────────┐
            │  loadr run consistency.yaml                                │
            │                                                            │
  cluster:  │   1. bring up / attach to the system under test (SUT)      │
            │                                                            │
  scenarios │   2. drive a CONCURRENT WORKLOAD with known op semantics   │
  +         │      (register / counter / set / bank / custom) over a     │
  consistency│     protocol plugin → every op recorded to a HISTORY      │
            │                                                            │
  nemesis:  │   3. on a SCHEDULE, inject FAULTS (partition / kill /      │
            │      pause / latency / clock skew) and record fault windows│
            │                                                            │
            │   4. CHECKER analyses the history against a chosen         │
            │      consistency MODEL → anomalies                         │
            │                                                            │
            │   5. REPORT: CAP classification + anomalies + availability │
            └──────────────────────────────────────────────────────────┘
```

The key insight that makes this fit loadr: an **operation** is just a request
whose result is recorded with **invoke** and **complete** real-times and a
**logical meaning** (e.g. "write key=x value=7", "read key=x → 5", "cas x 5→6
ok"). loadr already issues requests and timestamps samples; we add the logical
op semantics via the workload generator and a new `history` output, then feed
that to the checker.

---

## 4. New YAML surface

Three new top-level blocks, composing with everything that exists.

### 4.1 `cluster:` — the system under test

```yaml
cluster:
  name: mongo-rs
  # Either manage a cluster for the run…
  manage:
    compose: clusters/mongo-replicaset.yml   # or: kind/k8s manifest, or a script
    nodes: [mongo-1, mongo-2, mongo-3]        # logical node ids loadr can target
    ready: { exec: "mongosh --eval 'rs.status().ok'", timeout: 60s }
  # …or attach to an existing one:
  # attach:
  #   nodes:
  #     mongo-1: { host: 10.0.0.1, control: { via: toxiproxy, admin: 10.0.0.1:8474 } }
  control:
    via: toxiproxy            # how loadr injects faults: toxiproxy | docker | k8s | tc | exec
```

`control.via` abstracts the **fault mechanism**. `toxiproxy` (a real proxy in
front of each node) is the most portable for latency/partition; `docker`/`k8s`
for pause/kill; `tc`/`iptables` for kernel-level partitions; `exec` for an
arbitrary user script (`partition <a> <b>`, `heal`, `kill <node>`…).

### 4.2 `consistency:` — the workload model + what to verify

```yaml
consistency:
  workload: register          # register | counter | set | bank | custom
  model: linearizable         # linearizable | sequential | causal
                              # read-your-writes | monotonic-reads | serializable
  keys: 8                      # spread ops over N keys (per-key linearizability)
  # how each logical op maps onto the protocol plugin under test:
  bind:
    plugin: mongo
    write: { operation: update, collection: reg, filter: {k: "${key}"},
             update: { $set: { v: "${value}" } }, upsert: true }
    read:  { operation: find,   collection: reg, filter: {k: "${key}"},
             extract: { value: "$.[0].v" } }
    cas:   { operation: update, collection: reg,
             filter: {k: "${key}", v: "${expected}"},
             update: { $set: { v: "${value}" } } }   # acked-count == 1 ⇒ success
  checker:
    timeout: 60s              # bound the (potentially expensive) search
    external: none            # none | elle | knossos  (escape hatch)
```

The `bind:` mapping is what lets a single abstract workload run against **any**
protocol plugin — swap `plugin: mongo` for `postgres`/`redis`/`cassandra` and
the same register/linearizability test runs. Built-in workloads ship default
bindings for the first-party plugins so most users never write `bind:` by hand.

### 4.3 `nemesis:` — the fault schedule

```yaml
nemesis:
  seed: 1234                  # reproducible fault schedule
  faults:
    - { at: 20s, do: partition, groups: [[mongo-1], [mongo-2, mongo-3]], for: 25s }
    - { at: 60s, do: kill,      node: primary, for: 15s }   # primary resolved live
    - { every: 30s, do: latency, node: any, ms: 200, jitter: 50, for: 10s }
    - { at: 90s, do: clock-skew, node: mongo-2, offset: +30s, for: 20s }
  heal_at_end: true           # always return to a healthy cluster before checking
```

Faults: `partition` (split into groups, incl. one-way/asymmetric), `kill`,
`pause` (SIGSTOP), `restart`, `latency`, `packet-loss`, `clock-skew`,
`disk-fill` (later). `node: primary|any|<id>` is resolved against the cluster at
fire time. The schedule is seed-driven for reproducibility.

A run then ties these together with a normal scenario for the concurrency level:

```yaml
scenarios:
  hammer:
    executor: constant-vus
    vus: 50
    duration: 2m
    # flow is generated from `consistency.workload`; or write a custom flow
    # that emits history ops via the `history` output.
```

---

## 5. Components & architecture mapping

Everything below maps onto an existing loadr extension point, so this is
implementable without re-architecting the engine.

### 5.1 Nemesis driver — *new driver kind*
A `nemesis` controller that executes the fault schedule. Implemented behind a
trait with backends (`toxiproxy`, `docker`, `k8s`, `tc`, `exec`). It runs on the
**controller** (single source of truth for fault timing), records each fault as
a `(start, end, kind, target)` window into the run, and tags the metric stream
so the report can overlay faults. Reuses the existing docker-compose harness
machinery for `manage.compose`.

> Fits loadr's plugin model: a nemesis is a *service/control* plugin (the
> abi_stable plugin system already supports non-protocol plugin kinds), so
> third parties can add backends (chaos-mesh, AWS FIS, Pumba) out of tree.

### 5.2 Operation history — *new output `history`*
A structured, append-only log of operations:

```
{op_id, process(vu), key, f: read|write|cas, value|args,
 type: invoke|ok|fail|info, t_invoke_ns, t_complete_ns}
```

- `invoke`/`ok`/`fail` mirror Jepsen. `info` = **indeterminate** (timeout /
  connection reset) — critical: an indeterminate write may or may not have
  applied, and the checker must treat it as "possibly present".
- Built on the existing sample pipeline: the consistency workload emits these
  via `extras` on each request; the `history` output serialises them (and can
  stream to the checker). Real-times come from the controller's monotonic clock
  for single-node workloads (see §7).

### 5.3 Workload generators — *built-in flow templates + a `consistency` helper*
Canonical Jepsen-style workloads, each with known semantics the checker
understands:
- **register** — `read`/`write`/`cas` over N keys → **per-key linearizability**.
- **counter** — `add(delta)` + `read` → final read must equal sum of acked adds
  (and never exceed sum of all attempted incl. `info`).
- **set** — `add(e)` + `read-all` → every acked element must be present; no
  element that was never added may appear.
- **bank** — transfer between accounts in a txn → **total balance invariant** +
  no negative balances (a serializability smoke test).
- **custom** — author emits ops from JS/flow via the `history` output.

These are parameterised over a protocol plugin through the `bind:` mapping, so
they compose with the whole plugin ecosystem.

### 5.4 Consistency checker — *new crate `loadr-consistency`*
Runs post-run (and optionally streaming for early exit) over the history.
Pragmatic ladder, cheapest first:

1. **Invariant checks** (O(n), always on): counter total, set membership, bank
   total/non-negative, read-your-writes, monotonic reads/writes. These catch a
   large fraction of real bugs cheaply and need no expensive search.
2. **Per-key register linearizability** — Wing–Gong / competitive search
   (Porcupine-style) with a wall-clock `checker.timeout`. Per-key keeps the
   state space tractable; `info` ops modelled as may-have-happened.
3. **Transactional serializability** — **Elle-style** dependency-graph
   construction (ww/wr/rw edges from observed values) + cycle detection
   (G0 dirty writes, G1 dirty/intermediate reads, G2 anti-dependency cycles).
   Polynomial, scales far better than linearizability search; the right tool for
   SQL and document-txn stores.

Escape hatch: `checker.external: elle|knossos` exports the history in Jepsen's
EDN/JSON format and shells out, for users who want the reference checkers.

### 5.5 Report — *extends the existing HTML report*
- **CAP classification** banner (see §6).
- **Anomaly list**: each violation with the witnessing ops and real-times, e.g.
  *"stale read: op#1423 read x=v3 at t=41.2s, but op#1390 acked x=v5 at t=40.8s
  (linearizability violation)"*, or *"G2 cycle: T17 → T22 → T17"*.
- **Fault/availability/latency timeline**: the existing time-series chart with
  **fault windows shaded**, and a per-node/per-partition-side availability
  series so you can *see* the C-vs-A trade happen at the partition boundary.

---

## 6. The CAP classification output (the headline)

During each injected partition window loadr measures, on each side of the split:
- **Availability** — success rate of operations (reuses `*_req_failed`).
- **Consistency** — anomalies the checker attributes to that window.

and emits a verdict:

```
Partition [t=20s … 45s]  ([mongo-1] | [mongo-2,mongo-3])
  minority side (mongo-1):   availability 0.0%   (574 ops rejected)     → sacrificed A
  majority side:             availability 99.7%   0 consistency anomalies → preserved C
  VERDICT: CP under this partition (consistent, minority unavailable)

Re-run with w:1 / readPreference:nearest:
  minority side:             availability 98.9%                          → preserved A
  checker:                   12 stale reads, 1 lost update during window → sacrificed C
  VERDICT: AP under this partition (available, not linearizable)
```

That side-by-side — *same workload, same nemesis, two configs, opposite
verdicts* — is the product: it turns "we think Mongo with majority writes is
safe" into an evidenced statement, and it surfaces the cost (availability /
latency) of the safe setting.

---

## 7. Worked example — clustered MongoDB (the brief)

`clusters/mongo-replicaset.yml` (compose) brings up a 3-node replica set behind
toxiproxy. `mongo-cap.yaml`:

```yaml
name: mongo replica-set CAP test
plugins: [{ name: mongo }]

cluster:
  name: mongo-rs
  manage: { compose: clusters/mongo-replicaset.yml, nodes: [mongo-1, mongo-2, mongo-3],
            ready: { exec: "mongosh --quiet --eval 'rs.status().ok'" } }
  control: { via: toxiproxy }

consistency:
  workload: register
  model: linearizable
  keys: 16
  bind: { plugin: mongo }     # uses the built-in mongo register binding

nemesis:
  seed: 42
  faults:
    - { at: 20s, do: partition, groups: [[mongo-1], [mongo-2, mongo-3]], for: 25s }
    - { at: 70s, do: kill, node: primary, for: 20s }   # force a failover
  heal_at_end: true

scenarios:
  hammer: { executor: constant-vus, vus: 40, duration: 2m }

# you can still assert on the consistency result + availability SLOs:
thresholds:
  consistency_anomalies: ["count == 0"]      # fail the run if not linearizable
  mongo_req_failed:       ["rate < 0.05"]     # …unless you accept >5% unavailability
```

`loadr run mongo-cap.yaml` →
1. brings up the replica set, waits for `rs.status().ok`;
2. 40 VUs issue read/write/cas over 16 keys, recording history;
3. at 20s isolates the primary; at 70s kills the primary to force election;
4. heals, then checks per-key linearizability + read-your-writes;
5. prints the CAP verdict + anomalies + the shaded availability timeline, and
   the `thresholds` decide the exit code (CI-gateable).

Flip `bind` to `{ plugin: mongo, write_concern: 1, read_preference: nearest }`
and re-run to get the AP-flavoured verdict — same harness.

---

## 8. Consistency models supported

| Model | Workload | Checker | Cost |
|---|---|---|---|
| read-your-writes, monotonic reads/writes | register | invariant scan | O(n) |
| counter total / set membership / bank invariant | counter/set/bank | invariant scan | O(n) |
| **per-key linearizability** | register | Wing–Gong search (bounded) | exp, bounded by `timeout` + per-key |
| **serializability (G0/G1/G2)** | bank/txn | Elle cycle detection | polynomial |
| causal / sequential | register | (phase 4) version-graph checks | poly |

Start with the cheap invariants + Elle (high value, tractable); add bounded
linearizability for the register case; defer causal/sequential.

---

## 9. Distributed mode & the clock problem

Linearizability needs a trustworthy **real-time order** of invoke/complete. With
multiple loadr agents, clocks diverge and that order is unreliable. Options
(documented, user-selectable):
- **Single-process consistency workload (default):** run the consistency
  workload from the controller (or one agent) so all timestamps share one
  monotonic clock; use the agent fleet for *background* load/contention only.
  Linearizability is sound. This is what most Jepsen tests do.
- **Bounded-uncertainty mode:** distributed workload with NTP discipline; treat
  each op's real-time as an interval `[t±ε]`; the checker only flags violations
  that hold for *all* clocks within ε. Sound but weaker.
- **Weaker models only:** in fully distributed mode without clock discipline,
  restrict to checks that don't need real-time (set membership, counter bounds,
  causal via explicit version tokens). Validated by the config layer.

The config validator refuses `model: linearizable` in distributed mode unless a
clock-discipline option is chosen — fail loud, not silently-wrong.

---

## 10. Phasing / roadmap

- **Phase 1 — Chaos + availability (no checker).** `cluster:` + `nemesis:`
  (toxiproxy/docker backends) + fault-window metrics + shaded report timeline.
  Immediately useful: measure availability/latency under partition & failover.
- **Phase 2 — History + invariant workloads.** `history` output + register/
  counter/set/bank generators + O(n) invariant checks (RYW, monotonic, totals)
  + `consistency_anomalies` metric/threshold. Catches most real bugs cheaply.
- **Phase 3 — Real checkers (`loadr-consistency`).** Elle serializability +
  bounded per-key linearizability + the CAP-classification report.
- **Phase 4 — Lifecycle + advanced faults.** k8s/kind cluster management,
  clock-skew & asymmetric partitions, causal/sequential models, external-checker
  export (Elle/Knossos).

Each phase is independently shippable and demoable (a `loadr` plugin-style demo
video per phase, consistent with the rest of the project).

---

## 11. Risks & limitations
- **Checker cost.** General linearizability checking is NP-hard. Mitigate:
  per-key partitioning, bounded search with timeout, prefer Elle (polynomial)
  for transactions, surface "checked / timed-out / refuted" honestly.
- **Clock trust** in distributed mode (see §9) — don't claim soundness we can't
  back.
- **`info`/indeterminate ops** must be modelled, or the checker reports false
  violations. Non-negotiable correctness requirement of the history model.
- **Fault-mechanism fidelity.** toxiproxy partitions the proxy hop, not the
  kernel; `tc`/`iptables`/k8s are closer to real but need privilege. Document
  what each backend actually severs.
- **SUT access & privilege.** Injecting partitions/kills requires control over
  the cluster — gate behind explicit `cluster.control` config; never infer.
- **Reproducibility.** Seed the nemesis schedule *and* the workload RNG; record
  the seed in the report so a failing run replays.

## 12. Security
Fault injection is destructive by definition. Constraints, matching loadr's
security posture: `cluster`/`nemesis` are inert without an explicit
`cluster.control` block (no implicit host access); `exec` backends run only
user-provided, allow-listed commands (no raw interpolation of recorded data into
shell); managed clusters are namespaced/labelled so teardown can't touch
unrelated containers; and a `--dry-run` prints the fault schedule + cluster
actions without executing.

## 13. Open questions
1. Native Rust linearizability checker vs. always shelling to Elle/Knossos for
   the hard cases? (Lean: native invariants + Elle; native linearizability is a
   stretch goal.)
2. Should `cluster`/`nemesis` be **core** or a first-party **plugin**? (Lean:
   nemesis backends as plugins behind a core trait; history/checker in core.)
3. History storage for very long runs — cap, sample, or stream-to-disk + windowed
   checking?
4. Is a built-in `bank`/txn workload enough for SQL, or do we need user-defined
   transactional ops as a first-class step?
