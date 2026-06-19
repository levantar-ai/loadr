# Goals — concurrency & consistency testing ("loadr chaos")

Builds the feature specified in
[`docs/design/concurrency-consistency-testing.md`](../../docs/design/concurrency-consistency-testing.md),
which is the **authoritative spec** for every phase below.

Run the phases in order. Each is its own session goal and its own PR(s); each
must be green in CI before the next begins.

| Phase | Goal file | Outcome |
|-------|-----------|---------|
| 1 | [phase-1-chaos-and-availability.md](phase-1-chaos-and-availability.md) | `cluster:` + `nemesis:` + fault injection + availability/latency-under-fault, no checker yet |
| 2 | [phase-2-history-and-invariants.md](phase-2-history-and-invariants.md) | `history` output + register/counter/set/bank workloads + O(n) invariant checks |
| 3 | [phase-3-consistency-checkers.md](phase-3-consistency-checkers.md) | `loadr-consistency` crate: Elle serializability + bounded linearizability + CAP verdict |
| 4 | [phase-4-lifecycle-and-advanced.md](phase-4-lifecycle-and-advanced.md) | k8s lifecycle, clock-skew/asymmetric faults, causal/sequential, external-checker export |

## Shared quality bar (the canonical statement)

Every phase goal embeds a condensed copy of this so it is paste-ready on its
own. This is the full version and the source of truth.

**Spec & correctness**
- `docs/design/concurrency-consistency-testing.md` is authoritative; if reality
  forces a deviation, update the RFC in the same PR and say why.
- **TDD**: write tests with/before the implementation. No real network or live
  cluster in unit tests — use the seam/mock pattern; integration tests that need
  Docker are gated and clearly marked. Run with race detection.
- **Coverage**: keep workspace line coverage ≥ ~75% (the project floor); new
  logic paths are covered, especially checker/anomaly paths.
- **Honest reporting**: the checker and report must distinguish
  **checked / timed-out / refuted**; never claim a soundness we can't back
  (see the clock-discipline rule below). No silent truncation — if a history is
  sampled or a search is bounded, say so in the output.
- **Adversarial verification of the checker**: prove it catches a known-bad
  history (a hand-crafted anomaly) *and* clears a known-good one. A checker that
  only ever passes is worthless.

**Security** (matches the project's security-first posture)
- `cluster:`/`nemesis:` are inert without an explicit `cluster.control` block —
  never infer host/cluster access.
- `exec` fault backends run only allow-listed commands; never interpolate
  recorded/response data into a shell.
- Managed clusters are namespaced/labelled so teardown can't touch unrelated
  containers; `--dry-run` prints the cluster actions + fault schedule without
  executing.
- The config validator refuses `model: linearizable` in distributed mode unless
  a clock-discipline option is set (fail loud, not silently-wrong).

**Reproducibility**
- Seed the nemesis schedule *and* the workload RNG; record the seed in the
  report so a failing run replays.

**Engineering / deps**
- Pure-Rust where feasible; do not bloat the core binary. New fault backends are
  plugins behind a core trait; the history recorder and checker live in core /
  `loadr-consistency`.

**Commits & hooks**
- Conventional Commits (the commit-msg hook enforces it).
- **NEVER** use `--no-verify`. Run the lefthook pre-commit (fmt + clippy) and
  pre-push (tests + coverage) hooks; they must pass.
- End every commit message with:
  `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`

**Delivery & review**
- **Do not push to `main` directly** (the org blocks it). Open a PR per phase
  with `gh pr create`; merge with `gh pr merge --squash --delete-branch` only
  after **all** checks are green. Sync `main` between phases. Split a phase into
  smaller PRs if it gets large — but each PR must be green on its own.
- Use agents/worktrees to parallelise independent work within a phase, then
  review (adversarially for the checker) before merging.

**Docs, examples & demo** (parity with the rest of the project)
- Each phase ships a docs page wired into `docs/src/SUMMARY.md`, and a runnable
  example under `examples/`.
- Where there's something to *show* (a fault run, the CAP verdict), record a
  demo the way the other features do: commit the vhs `.tape` / Playwright recipe
  (the `.mp4`/poster are produced into the gitignored `site/videos/out/` for
  `deploy.sh`).

**Proof of done**
- Actually run the phase's example end-to-end (Docker/toxiproxy required) and
  paste the output as proof before declaring the phase done. Then tick the phase
  in the RFC with a link to its merged PR.
