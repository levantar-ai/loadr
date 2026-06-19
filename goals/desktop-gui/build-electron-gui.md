# Goal — loadr Desktop (Electron GUI)

A full desktop GUI that drives the `loadr` CLI underneath: compose / manage /
save / run plans with results, tabbed per plan, drag-and-drop plan composition,
and faithful **round-trip** between an existing YAML plan and the UI. Cross-
platform build artifacts, CI, and semantic-release.

Paste this whole block into a fresh Claude Code session in this repo:

```text
/goal Build "loadr Desktop", an Electron GUI for loadr, in a new `desktop/` workspace in this repo. It drives the loadr CLI underneath and gives a complete UI to compose, manage, save and run test plans with live results. Treat the loadr YAML schema (docs + `loadr schema` JSON Schema + the loadr-config crate) as the source of truth.

ARCHITECTURE
- Electron + TypeScript. Renderer in this repo's frontend stack: React 19, Vite 6, Tailwind 4, ESLint 9 flat config, Vitest 3 + Testing Library. Secure defaults: contextIsolation on, nodeIntegration off, a typed preload bridge; the renderer never spawns processes directly.
- Main process spawns a BUNDLED `loadr` binary (per-platform) via IPC — the GUI is a front-end over the CLI, not a reimplementation. At build time, fetch/stage the matching-version loadr binary for each target OS/arch and package it as an app resource; resolve it at runtime (bundled first, then PATH). Pin the loadr version the app was built against.
- Plan model is SCHEMA-DRIVEN: generate/validate forms from loadr's JSON Schema (`loadr schema`), and validate plans by calling the loadr binary (`loadr validate`/parse) so the GUI and CLI never disagree.

FEATURES (all required)
- Tabbed workspace: one tab per open plan, dirty indicators, multiple plans open at once, reorderable tabs.
- Plan composer (forms): edit defaults, variables, secrets refs, scenarios+executors, flow steps, thresholds, plugins — every field schema-validated with inline errors.
- DRAG-AND-DROP composition (dnd-kit): a palette of composable blocks (request, think_time, group, parallel, repeat, while, if, foreach, switch, retry, rendezvous, extract, assert/checks) that you drag into a scenario's flow; reorder by drag; nest into groups/parallel; keyboard-accessible dnd.
- ROUND-TRIP both ways: open an existing .yaml → render the full UI from it; edit in the UI → save back to YAML that `loadr validate` accepts clean. Include a raw YAML view (Monaco) two-way synced with the form view. This fidelity is the make-or-break of the app — see TESTS.
- Plan management: workspace/library of plans, recent files, open/save/save-as/duplicate, starter templates, and IMPORT via `loadr convert` (jmx / k6 / har) directly in the UI.
- Run + results: run a plan (spawn loadr), stream LIVE metrics (VUs, RPS, p95, error rate) parsed from loadr's JSON output, then full results — throughput, latency percentiles, error-rate timeline, thresholds pass/fail, checks. Persist run history per plan and allow comparing two runs.
- Plugins panel: list/search/install/remove via `loadr plugin …`.

PACKAGING / CI / RELEASE
- electron-builder targets: macOS (dmg + zip, x64 + arm64), Windows (nsis + portable), Linux (AppImage + deb). Bundle the loadr binary per target. Wire code-signing + macOS notarization but gate them on CI secrets (documented; build unsigned when absent).
- GitHub Actions matrix (macos-latest, windows-latest, ubuntu-latest): install, lint, typecheck, unit tests, build, Playwright-for-Electron e2e (xvfb on Linux), then package and upload artifacts.
- semantic-release driven by Conventional Commits → version bump → build all platforms → publish artifacts to GitHub Releases with SLSA provenance, mirroring this repo's existing release setup. NOTE: the org disables GITHUB_TOKEN write for releases — use the PAT_TOKEN secret, as the Rust release workflow does.

QUALITY BAR (non-negotiable)
- TDD. THE critical tests: round-trip property tests proving `parse(render(plan)) === plan` for a corpus of real plans (including the repo's examples/) AND that any sequence of UI edits serialises to YAML that `loadr validate` accepts. A drag-drop that emits invalid YAML is a bug.
- Playwright-for-Electron e2e covering the headline flows: open an example plan → it renders; drag a request + an extract into a flow → save → `loadr validate` passes; run a plan → live metrics then results render; import a .har → plan opens. Runs headless in CI.
- Accessibility: keyboard-operable drag-drop and forms; focus management; labelled controls.
- Security: contextIsolation/preload only; spawn only the bundled/allow-listed loadr binary with array args (no shell string interpolation of user input); never eval plan content.
- Conventional Commits; NEVER --no-verify; lint/typecheck/test hooks must pass; end every commit with `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`.
- Do NOT push to main (org blocks it): PR per milestone via `gh pr create`, merge `--squash --delete-branch` only when ALL checks are green; sync main between milestones. Use agents/worktrees for parallel work, then review.
- Docs: a `desktop/README.md` (dev + build + release) and a user-facing docs page wired into docs/src/SUMMARY.md; a committed demo recording (Playwright/vhs) of composing + running a plan (mp4/poster into the gitignored site/videos/out/).

SUGGESTED MILESTONES (each its own green PR): (1) scaffold Electron+React+Vite+TS+Tailwind, IPC bridge, bundled-loadr spawn, "open YAML → render read-only" + round-trip tests; (2) full form editor + Monaco two-way sync + schema validation; (3) drag-and-drop composition + tabs + save/manage/import; (4) run + live results + history/compare; (5) plugins panel; (6) electron-builder packaging + CI matrix + Playwright e2e; (7) semantic-release + signed multi-platform artifacts + docs + demo.

DONE when: every milestone is merged to main green; CI builds and packages installable artifacts for macOS (x64+arm64), Windows and Linux; the Playwright-for-Electron e2e suite passes (open → drag-compose → save → validate → run → results, and .har import); the round-trip property tests pass over examples/; semantic-release publishes versioned artifacts on a release commit; and docs + a demo recording exist. Paste e2e output + the artifact list as proof, and record any blocker (e.g. signing secrets, no display in CI) in desktop/README.md rather than stopping — finish all unblocked work + tests.
```

A few notes:

- **Scope is large** — it's a real desktop app. The milestone list inside the prompt is so a single loop can grind through it as sequential green PRs; if you'd rather keep tight control, set the goal to **Milestone 1 only** first (scaffold + the round-trip engine), since that's the riskiest, most load-bearing piece — get YAML↔UI fidelity proven before building the rest on top.
- **It reuses your stack deliberately** (React 19/Vite 6/Tailwind 4, semantic-release, SLSA, PAT_TOKEN) so it slots into the repo's conventions rather than inventing new ones.
- **The round-trip property tests are the heart of it** — I weighted the prompt around them because "render UI from an existing plan" + "save back to valid YAML" is exactly where these editors usually rot.

Want me to also split this into per-milestone goal files (like the chaos feature), or kick off Milestone 1 right now so you can see the round-trip engine working before committing to the whole build?

