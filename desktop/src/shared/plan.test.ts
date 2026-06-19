// Round-trip property tests — the make-or-break of the GUI.
//
// For every plan in the repo's examples/ corpus we assert:
//   1. parse → serialize → parse preserves the plan's data (no loss), and
//   2. the serialized YAML is accepted by `loadr validate` (the CLI is the
//      source of truth — the GUI must never emit a plan the CLI rejects).
//
// (2) needs a loadr binary; it's resolved from $LOADR_BIN, then the workspace
// debug/release build, then PATH. When none is found those assertions are
// skipped with a warning (structural round-trip still runs) so the suite works
// in a JS-only CI lane; locally/CI-with-loadr it's the real proof.

import { execFileSync } from 'node:child_process';
import { existsSync, mkdtempSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

import { describe, expect, it } from 'vitest';

import { deepEqualUnordered, parsePlan, serializePlan } from './plan';

const examplesDir = fileURLToPath(new URL('../../../examples', import.meta.url));

function resolveLoadr(): string | null {
  if (process.env.LOADR_BIN && existsSync(process.env.LOADR_BIN)) return process.env.LOADR_BIN;
  for (const rel of ['../../../target/release/loadr', '../../../target/debug/loadr']) {
    const p = fileURLToPath(new URL(rel, import.meta.url));
    if (existsSync(p)) return p;
  }
  try {
    return execFileSync('bash', ['-lc', 'command -v loadr']).toString().trim() || null;
  } catch {
    return null;
  }
}

const loadr = resolveLoadr();
const exampleFiles = readdirSync(examplesDir)
  .filter((f) => f.endsWith('.yaml') || f.endsWith('.yml'))
  .sort();

describe('plan round-trip', () => {
  it('finds the examples corpus', () => {
    expect(exampleFiles.length).toBeGreaterThan(10);
  });

  it.each(exampleFiles)('preserves data through parse→serialize→parse: %s', (file) => {
    const src = readFileSync(join(examplesDir, file), 'utf8');
    const plan = parsePlan(src);
    const round = parsePlan(serializePlan(plan));
    expect(deepEqualUnordered(plan, round)).toBe(true);
  });

  it.runIf(loadr !== null).each(exampleFiles)(
    'serialized YAML is accepted by `loadr validate`: %s',
    (file) => {
      const src = readFileSync(join(examplesDir, file), 'utf8');
      const out = serializePlan(parsePlan(src));
      const dir = mkdtempSync(join(tmpdir(), 'loadr-rt-'));
      const tmp = join(dir, file);
      writeFileSync(tmp, out);
      // Exit 0 == no validation errors. --no-check-files: referenced CSV/JS/
      // protos live next to the original, not our temp copy.
      expect(() =>
        execFileSync(loadr!, ['validate', '--no-check-files', tmp], { stdio: 'pipe' }),
      ).not.toThrow();
    },
  );
});

describe('parse errors', () => {
  it('rejects a non-mapping top level', () => {
    expect(() => parsePlan('- just\n- a\n- list')).toThrow();
  });
  it('treats empty input as an empty plan', () => {
    expect(parsePlan('')).toEqual({});
  });
});
