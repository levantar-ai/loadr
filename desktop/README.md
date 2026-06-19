# loadr Desktop

A desktop GUI for [loadr](https://loadr.io): compose, manage, save and run test
plans with live results. The app is a **front-end over the loadr CLI** — it
spawns a bundled `loadr` binary for validation, schema, running and plugins, so
the GUI and the CLI can never disagree about what a plan means.

Stack: Electron + TypeScript, React 19 + Vite 6 + Tailwind 4 (renderer), Vitest
(unit), Playwright-for-Electron (e2e), electron-builder (packaging).

## Status — built in milestones (see `goals/desktop-gui/`)
- [x] **M1** — scaffold, secure IPC, bundled-loadr bridge, open-YAML→render, round-trip tests
- [ ] M2 — schema-driven form editor + Monaco two-way sync
- [ ] M3 — drag-and-drop composition + tabs + manage/import
- [ ] M4 — run + live results + history/compare
- [ ] M5 — plugins panel
- [ ] M6 — electron-builder packaging + CI matrix + Playwright e2e
- [ ] M7 — semantic-release + signed multi-platform artifacts

## Develop
```bash
cd desktop
npm install
npm run dev          # launch the app (needs a display)
npm test             # unit + round-trip tests (headless)
npm run typecheck
npm run lint
```

### The loadr binary
The app resolves loadr in this order: **bundled** (`resources/bin/loadr` in a
packaged build) → `$LOADR_BIN` → `PATH`. For local dev, either `cargo build
-p loadr-cli` (the app/tests find `../target/{debug,release}/loadr`) or set
`LOADR_BIN`. The round-trip tests that call `loadr validate` are skipped if no
binary is found (the structural round-trip still runs).

## Round-trip guarantee
`src/shared/plan.ts` parses YAML → model and serializes model → YAML.
`src/shared/plan.test.ts` proves, over the repo's `examples/` corpus, that
parse→serialize→parse preserves the plan's data **and** that the serialized YAML
is accepted by `loadr validate`. A GUI edit that produces invalid YAML is a bug.

## Known blockers (environment-dependent)
- **Running the Electron window / Playwright-for-Electron e2e** needs a display
  (use `xvfb-run` in headless CI). The Vitest round-trip suite is fully headless.
- **Code-signing / macOS notarization** (M7) needs Apple/Windows certs supplied
  as CI secrets; builds are produced unsigned when the secrets are absent.
