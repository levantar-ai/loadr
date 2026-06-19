# Phase 4 — Lifecycle & advanced faults

Hardening and reach: real cluster lifecycle (k8s/kind), clock-skew & asymmetric
partitions, causal/sequential models, and external-checker export. Spec: §4.1
(k8s control), §4.3 (clock-skew), §8 (causal/sequential rows), §10 (Phase 4),
§13 open questions. **Requires Phase 3 merged.**

Paste this whole block into a fresh Claude Code session:

```text
/goal Implement Phase 4 of docs/design/concurrency-consistency-testing.md — cluster lifecycle + advanced faults + weaker models + external-checker export. Phases 1–3 must already be merged. The RFC is authoritative (§4.1, §4.3, §8 causal/sequential, §10 Phase 4, §13).

Build:
- A `k8s`/`kind` cluster-control backend (behind the Phase-1 trait): bring up/attach to a cluster, resolve nodes, and inject partition/kill/pause/latency via the k8s API (or chaos-mesh if present), in addition to the existing toxiproxy/docker backends.
- A `clock-skew` nemesis fault and asymmetric/one-way partitions; resolve targets live; record windows like the other faults.
- Weaker consistency models: causal and sequential consistency checks (version-graph / happens-before via explicit version tokens), usable where linearizability is unavailable (incl. clock-disciplined distributed mode). Wire into the §8 model table.
- Finalise the external-checker export path (`checker.external: elle|knossos`): a documented, stable history format + an allow-listed shell-out, with a round-trip test.
- A distributed/bounded-uncertainty mode for real-time (§9): per-op real-time intervals [t±ε] under NTP discipline; the checker only flags violations that hold for all clocks within ε.

QUALITY BAR (non-negotiable):
- TDD; known-good/known-bad fixtures for the causal/sequential checks (catch a causality violation, clear a causal history); ≥ ~75% coverage; no real cluster in unit tests (gate k8s integration tests, mark clearly).
- Honest reporting: causal/sequential results report checked/refuted/timed-out and state their assumptions; bounded-uncertainty mode states ε and only flags all-clocks violations.
- SECURITY: k8s/exec backends inert without explicit cluster.control; allow-listed commands only; namespaced teardown; `--dry-run` covers the new backends + clock-skew.
- Reproducibility: seeds for the new faults; printed in the report.
- Conventional Commits; NEVER --no-verify; lefthook pre-commit + pre-push must pass; end every commit with `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`.
- Do NOT push to main: PR via `gh pr create`, merge `--squash --delete-branch` only when ALL checks green; sync main first; agents/worktrees for parallel work then review.
- Docs pages wired into docs/src/SUMMARY.md (k8s setup, advanced faults, weaker models, external checkers); a runnable k8s/kind example; committed vhs `.tape` demo of a clock-skew or k8s-partition run.
- Resolve the §13 open questions in the RFC (record the decisions taken).

DONE when: the PR(s) are merged green; an example runs a partition/clock-skew fault against a kind/k8s cluster and produces a verdict (paste output); the causal/sequential known-bad fixtures refute and known-good clear; the external-checker export round-trips; §13 open questions are answered in the RFC and all four phases are ticked as shipped with PR links. Record any blocker (e.g. no k8s available) in the RFC and finish all unblocked work + tests rather than stopping.
```

**Proof of done:** merged green PR(s) + a k8s/clock-skew run output + causal
refutation tests + external-checker round-trip + RFC fully ticked and §13
answered.
