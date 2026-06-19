# Phase 2 — Operation history & invariant workloads

Records a structured operation history and adds the canonical Jepsen-style
workloads plus cheap O(n) invariant checks — catches a large fraction of real
concurrency bugs without the expensive checker. Spec: §4.2, §5.2, §5.3, §8 (the
invariant rows), §9. **Requires Phase 1 merged.**

Paste this whole block into a fresh Claude Code session:

```text
/goal Implement Phase 2 of docs/design/concurrency-consistency-testing.md — the operation history + workload generators + O(n) invariant checks. Phase 1 (cluster/nemesis) must already be merged. The RFC is authoritative (§4.2 consistency block, §5.2 history output, §5.3 workloads, §8 invariant rows, §9 clock note).

Build:
- A `history` output: an append-only log of operations {op_id, process(vu), key, f (read|write|cas|add|transfer), value/args, type (invoke|ok|fail|info), t_invoke_ns, t_complete_ns}. `info` = indeterminate (timeout/reset) and MUST be representable. Build it on the existing sample pipeline (ops emitted via request `extras`); real-times from the controller's monotonic clock for single-process workloads. Support streaming to disk for long runs.
- A `consistency:` plan block: `workload` (register|counter|set|bank), `model`, `keys`, and a `bind:` mapping that expresses each logical op (read/write/cas/add/transfer) onto a protocol plugin. Ship default bindings for the first-party plugins (mongo, postgres, redis) so `bind:` is optional for them.
- The workload generators: register (read/write/cas over N keys), counter (add/read), set (add/read-all), bank (transfer in a txn). Each emits history ops with correct semantics and is parameterised over the bound plugin.
- O(n) invariant checks + a `consistency_anomalies` metric (threshold-gateable): read-your-writes, monotonic reads/writes, counter total (final == sum of acked adds; never exceeds sum incl. info), set membership (every acked element present; nothing un-added appears), bank total-invariant + non-negative. Each anomaly carries the witnessing ops + real-times.
- Wire it together with a nemesis run from Phase 1, and report anomalies on the timeline alongside fault windows.

QUALITY BAR (non-negotiable):
- TDD; no real network in unit tests; cover every invariant with BOTH a passing and a deliberately-violating history fixture; ≥ ~75% coverage.
- Honest reporting: distinguish checked/timed-out/refuted; never silently drop history (if sampled, say so). Model `info` ops as may-have-happened — a missed `info` must not produce a false violation; test this explicitly.
- Config validator refuses `model: linearizable` in distributed mode without a clock-discipline option (§9); the cheap invariant models are allowed in distributed mode.
- SECURITY: no shell interpolation of recorded data; the consistency workload inherits Phase-1 cluster.control gating.
- Reproducibility: seed the workload RNG; print the seed.
- Conventional Commits; NEVER --no-verify; lefthook pre-commit + pre-push must pass; end every commit with `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`.
- Do NOT push to main: PR via `gh pr create`, merge `--squash --delete-branch` only when ALL checks green; sync main first; agents/worktrees for parallel work then review.
- Docs page wired into docs/src/SUMMARY.md, a runnable example per workload, and a committed vhs `.tape` demo showing a counter/bank run detecting (or clearing) an invariant under a partition.

DONE when: the PR is merged green; `loadr run <a counter or bank example>` runs the workload under a Phase-1 partition/kill, records a history, and reports the invariant result (and `consistency_anomalies` gates a threshold) — paste output for one passing and one intentionally-broken config; the `info`-handling test passes; and Phase 2 is ticked in the RFC with a PR link. Record any blocker in the RFC and finish unblocked work + tests rather than stopping.
```

**Proof of done:** merged green PR + pasted output for a passing run and an
intentionally-broken run + the `info`-op false-positive test + RFC box ticked.
