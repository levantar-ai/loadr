# goals/

Paste-ready **`/goal`** prompts for Claude Code — one self-contained block per
unit of work. A goal sets a session-scoped Stop hook that blocks the session
from stopping until its **Done** condition holds, so each file is written with a
concrete, checkable completion state.

## How to use
1. Open a fresh Claude Code session in this repo.
2. Open the goal file for the work you want, copy the fenced `/goal …` block,
   and paste it as your first message.
3. Run phases **in order** — each builds on the last.

## Conventions
- One directory per feature (e.g. `concurrency-consistency-testing/`).
- One goal file per phase: `phase-N-<slug>.md`.
- Every goal file is **standalone** — it embeds the full quality bar so it works
  when pasted on its own, and names its authoritative spec.
- The feature directory's `README.md` is the canonical statement of the shared
  quality bar and the phase ordering.
