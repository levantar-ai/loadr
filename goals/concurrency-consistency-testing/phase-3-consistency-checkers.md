# Phase 3 — Real consistency checkers + CAP verdict

The hard part: a `loadr-consistency` crate with Elle-style serializability and
bounded per-key linearizability, plus the CAP-classification report. Spec: §5.4,
§6, §8 (the search rows), §11. **Requires Phase 2 merged.**

Paste this whole block into a fresh Claude Code session:

```text
/goal Implement Phase 3 of docs/design/concurrency-consistency-testing.md — the consistency checkers and CAP-classification report. Phases 1–2 must already be merged. The RFC is authoritative (§5.4 checker, §6 CAP verdict, §8 search rows, §11 risks).

Build a new crate `loadr-consistency` consuming the Phase-2 history:
- Elle-style transactional serializability: build ww/wr/rw dependency edges from observed values and detect cycles (report G0 dirty write, G1a/G1b dirty/intermediate read, G2 anti-dependency). Polynomial; the primary tool for SQL/document-txn stores.
- Bounded per-key register linearizability: a Wing–Gong / competitive search, partitioned per key, honouring `checker.timeout`. Model `info` (indeterminate) ops as may-have-happened. On timeout, report `timed-out` for that key — never a false pass or fail.
- CAP classification (§6): for each partition window, compute availability per side and attribute anomalies to the window, then emit a CP-vs-AP verdict with the witnessing evidence.
- Anomaly reporting in the HTML report: each violation with the witnessing ops + real-times (e.g. "stale read: op#X read v3 at t while op#Y acked v5 at t'"); the CAP verdict banner; faults/availability/anomalies on one timeline.
- Escape hatch: `checker.external: elle|knossos` exports the history in Jepsen-compatible EDN/JSON and shells out (allow-listed command only).

QUALITY BAR (non-negotiable):
- TDD with a fixture suite of KNOWN-GOOD and KNOWN-BAD histories: the linearizability checker must catch a hand-crafted stale-read/lost-update and clear a valid one; the Elle checker must catch a constructed G2 cycle and clear an acyclic history. A checker that only ever passes is a bug — assert it refutes.
- Honest reporting is the headline: every result is checked | timed-out | refuted; the report states bounds (per-key, timeout) plainly; no silent truncation.
- Clock soundness (§9): linearizability results are only emitted for single-process (or clock-disciplined) histories; the validator already enforces this from Phase 2 — add a checker-side guard too.
- Performance: per-key partitioning + the timeout keep it tractable; document complexity; prefer Elle for transactions. Pure-Rust; no heavy/un-cross-compilable deps in core.
- Conventional Commits; NEVER --no-verify; lefthook pre-commit + pre-push must pass; ≥ ~75% coverage; end every commit with `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`.
- Do NOT push to main: PR via `gh pr create`, merge `--squash --delete-branch` only when ALL checks green; sync main first. Use agents/worktrees for parallel work and ADVERSARIALLY review the checker (independent verification that it refutes bad histories) before merging.
- Docs page wired into docs/src/SUMMARY.md; finish the §7 worked example (clusters/mongo-replicaset.yml + examples/.../mongo-cap.yaml) so it runs end to end; committed vhs `.tape` demo of the CAP verdict.

DONE when: the PR is merged green; `loadr run examples/.../mongo-cap.yaml` brings up the replica set, injects a partition + primary kill, runs the linearizability check, and prints a CAP verdict (CP vs AP) + an anomaly list + the fault/availability timeline — and a `w:1` variant produces the opposite (AP) verdict (paste both); `consistency_anomalies` gates the run's exit code; the known-bad/known-good checker fixtures pass; and Phase 3 is ticked in the RFC with a PR link. Record any blocker in the RFC and finish unblocked work + tests rather than stopping.
```

**Proof of done:** merged green PR + the §7 example producing CP *and* AP
verdicts on the two configs + the known-bad-history refutation tests + RFC box
ticked.
