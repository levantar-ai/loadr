# Phase 1 — Chaos & availability (no checker yet)

Stands up the cluster + fault-injection machinery and measures availability and
latency under faults. Independently valuable and de-risks the orchestration
before the hard checker work. Spec: §4.1, §4.3, §5.1, §5.5 of the RFC.

Paste this whole block into a fresh Claude Code session:

```text
/goal Implement Phase 1 of docs/design/concurrency-consistency-testing.md — cluster orchestration + fault injection + availability-under-fault. That RFC is the authoritative spec (§4.1 cluster, §4.3 nemesis, §5.1 nemesis driver, §5.5 report).

Build:
- A `cluster:` plan block: `manage` (bring up via docker-compose; wait on a `ready` probe) or `attach` (existing nodes), plus `control.via` (toxiproxy + docker backends for now) behind a core trait so more backends can be added as plugins later.
- A `nemesis:` plan block: a seeded, reproducible fault schedule — `partition` (incl. asymmetric/one-way), `kill`, `pause`, `restart`, `latency`, `packet-loss` — with `at`/`every`/`for` timing and `node: primary|any|<id>` resolved live. Implement `partition`/`latency`/`packet-loss` via toxiproxy and `kill`/`pause`/`restart` via docker.
- A nemesis controller that runs on the controller process, executes the schedule, and records each fault as a `(start,end,kind,target)` window; tag the metric stream so the report can overlay them.
- Report: shade fault windows on the existing HTML time-series chart and add a per-node / per-partition-side availability series (reuse `*_req_failed`). Print, per fault window, the availability + latency on each side.
- `clusters/mongo-replicaset.yml` (3-node replica set behind toxiproxy) and an example plan that drives a simple mongo workload while injecting a partition then a primary kill, and reports availability/latency per window.

QUALITY BAR (non-negotiable):
- TDD; no real network/cluster in unit tests (seam/mock the backend trait); Docker-gated integration tests clearly marked; workspace line coverage ≥ ~75%.
- SECURITY: `cluster`/`nemesis` inert without an explicit `cluster.control` block; `exec`-style backends allow-listed (no shell interpolation of data); managed clusters namespaced/labelled so teardown is safe; add `--dry-run` that prints the cluster actions + fault schedule without executing.
- Reproducibility: seed the schedule; print the seed.
- Pure-Rust where feasible; don't bloat core; backends behind the core trait.
- Conventional Commits; NEVER --no-verify; lefthook pre-commit (fmt/clippy) + pre-push (tests/coverage) must pass; end every commit with `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`.
- Do NOT push to main (org blocks it): open a PR with `gh pr create`, merge `--squash --delete-branch` only after ALL checks are green; sync main first. Use agents/worktrees for parallel work, then review.
- Ship a docs page wired into docs/src/SUMMARY.md, the example above, and a committed vhs `.tape` demo of a fault run (mp4/poster into the gitignored site/videos/out/).

DONE when: the PR is merged to main green; `loadr run <the example>` brings up the replica set, injects a partition + a primary kill on a seeded schedule, and prints per-window availability + latency with the report timeline shading the fault windows (paste the run output as proof); `--dry-run` prints the plan without touching anything; and docs/design/concurrency-consistency-testing.md is updated to mark Phase 1 shipped with a link to the PR. If genuinely blocked (e.g. no Docker), record the blocker in that doc and finish all unblocked work + tests rather than stopping.
```

**Proof of done:** merged green PR + pasted run output showing fault-windowed
availability/latency + `--dry-run` output + RFC Phase-1 box ticked.
