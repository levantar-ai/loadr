// Main-process bridge to the loadr CLI. The GUI is a front-end over the CLI:
// we resolve a bundled, version-pinned binary (falling back to PATH in dev) and
// invoke it with ARRAY args only — never a shell string — so plan content can
// never be interpreted by a shell.

import { execFile, spawn } from 'node:child_process';
import { existsSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { promisify } from 'node:util';

import { parseSummary, type Summary } from '../shared/results';
import { parsePluginList, type InstalledPlugin } from '../shared/plugins';
import { cliError } from './errors';

const execFileP = promisify(execFile);

/** Where the bundled binary lives inside a packaged app, relative to resources. */
function bundledPath(): string | null {
  // electron sets process.resourcesPath in packaged apps.
  const res = (process as NodeJS.Process & { resourcesPath?: string }).resourcesPath;
  if (!res) return null;
  const exe = process.platform === 'win32' ? 'loadr.exe' : 'loadr';
  const p = join(res, 'bin', exe);
  return existsSync(p) ? p : null;
}

/** Resolve the loadr binary: bundled first, then $LOADR_BIN, then PATH. */
export function resolveLoadr(): string {
  return bundledPath() ?? process.env.LOADR_BIN ?? 'loadr';
}

export interface Diagnostic {
  severity: 'error' | 'warning' | string;
  message: string;
  [k: string]: unknown;
}

export interface ValidateResult {
  ok: boolean; // no errors
  diagnostics: Diagnostic[];
  raw: string;
}

/** `loadr --version`. */
export async function version(): Promise<string> {
  const bin = resolveLoadr();
  try {
    const { stdout } = await execFileP(bin, ['--version']);
    return stdout.trim();
  } catch (e) {
    throw cliError(e, bin);
  }
}

/** A health check the UI can run on startup to diagnose a broken engine. */
export interface Health {
  ok: boolean;
  path: string;
  version?: string;
  problem?: string;
}
export async function doctor(): Promise<Health> {
  const path = resolveLoadr();
  try {
    return { ok: true, path, version: await version() };
  } catch (e) {
    return { ok: false, path, problem: (e as Error).message };
  }
}

/** The plan JSON Schema (`loadr schema`) — drives schema-aware form rendering. */
export async function schema(): Promise<unknown> {
  const bin = resolveLoadr();
  try {
    const { stdout } = await execFileP(bin, ['schema'], { maxBuffer: 32 * 1024 * 1024 });
    return JSON.parse(stdout);
  } catch (e) {
    throw cliError(e, bin);
  }
}

/**
 * Validate a plan's YAML by handing it to the CLI. Writes to a temp file (the
 * CLI validates paths) and runs `loadr validate --format json`. Errors are
 * returned as diagnostics, not thrown — invalid plans are an expected state in
 * an editor.
 */
export async function validate(yamlText: string, checkFiles = false): Promise<ValidateResult> {
  const dir = mkdtempSync(join(tmpdir(), 'loadr-validate-'));
  const file = join(dir, 'plan.yaml');
  writeFileSync(file, yamlText);
  const args = ['validate', '--format', 'json'];
  if (!checkFiles) args.push('--no-check-files');
  args.push(file);
  try {
    const { stdout } = await execFileP(resolveLoadr(), args, { maxBuffer: 16 * 1024 * 1024 });
    return parseValidate(stdout);
  } catch (e) {
    // Non-zero exit (validation errors) still carries JSON on stdout.
    const stdout = (e as { stdout?: string }).stdout ?? '';
    if (stdout) return parseValidate(stdout);
    // No stdout means the engine couldn't run at all — surface why, in English.
    return {
      ok: false,
      diagnostics: [{ severity: 'error', message: cliError(e, resolveLoadr()).message }],
      raw: stdout,
    };
  }
}

/** Map a file extension to a `loadr convert --from` kind. */
export function convertKind(file: string): 'jmx' | 'k6' | 'har' | null {
  const ext = file.split('.').pop()?.toLowerCase();
  if (ext === 'jmx' || ext === 'xml') return 'jmx';
  if (ext === 'js' || ext === 'ts' || ext === 'mjs') return 'k6';
  if (ext === 'har') return 'har';
  return null;
}

/** Installed plugins (`loadr plugin list`). */
export async function pluginList(): Promise<InstalledPlugin[]> {
  try {
    const { stdout } = await execFileP(resolveLoadr(), ['plugin', 'list']);
    return parsePluginList(stdout);
  } catch {
    return [];
  }
}

/** Install a plugin by index name, directory or URL. Returns CLI output. */
export async function pluginInstall(spec: string, allowUntrusted = false): Promise<string> {
  const bin = resolveLoadr();
  const args = ['plugin', 'install', spec];
  if (allowUntrusted) args.push('--allow-untrusted');
  try {
    const { stdout, stderr } = await execFileP(bin, args, { maxBuffer: 16 * 1024 * 1024 });
    return (stdout + stderr).trim();
  } catch (e) {
    throw cliError(e, bin);
  }
}

/** Remove an installed plugin by name. */
export async function pluginRemove(name: string): Promise<void> {
  const bin = resolveLoadr();
  try {
    await execFileP(bin, ['plugin', 'remove', name]);
  } catch (e) {
    throw cliError(e, bin);
  }
}

/** Import a JMeter/k6/HAR file via `loadr convert`; returns the YAML it emits. */
export async function convert(file: string): Promise<string> {
  const kind = convertKind(file);
  if (!kind) throw new Error(`cannot import ${file}: expected .jmx, .js or .har`);
  const bin = resolveLoadr();
  try {
    const { stdout } = await execFileP(bin, ['convert', '--from', kind, file], {
      maxBuffer: 32 * 1024 * 1024,
    });
    return stdout;
  } catch (e) {
    throw cliError(e, bin);
  }
}

/**
 * Run a plan: spawn `loadr run <plan> --summary-export <json>`, stream each
 * progress/log line to `onLine` (loadr uses \r for the live line, so we split
 * on either), and resolve with the parsed end-of-run summary. Array args only.
 */
/** A completed run: the parsed summary plus the CLI's JUnit XML report. */
export interface RunResult {
  summary: Summary;
  junit: string;
}

export function runPlan(
  yamlText: string,
  onLine: (line: string) => void,
  onChild?: (child: ReturnType<typeof spawn>) => void,
): Promise<RunResult> {
  const dir = mkdtempSync(join(tmpdir(), 'loadr-run-'));
  const planPath = join(dir, 'plan.yaml');
  const summaryPath = join(dir, 'summary.json');
  const junitPath = join(dir, 'junit.xml');
  writeFileSync(planPath, yamlText);

  const bin = resolveLoadr();
  return new Promise<RunResult>((resolve, reject) => {
    // The CLI writes the JUnit report itself, so the GUI and CI produce byte-for-
    // byte identical reports — the renderer never re-derives it.
    const child = spawn(
      bin,
      ['run', planPath, '--summary-export', summaryPath, '--junit', junitPath],
      { stdio: ['ignore', 'pipe', 'pipe'] },
    );
    onChild?.(child);
    let tail = ''; // bounded copy of recent output, for a failure message
    const pump = (buf: Buffer) => {
      const text = buf.toString();
      tail = (tail + text).slice(-2000);
      for (const line of text.split(/[\r\n]+/)) {
        if (line.trim()) onLine(line);
      }
    };
    child.stdout.on('data', pump);
    child.stderr.on('data', pump);
    // Spawn failed entirely (missing binary, wrong CPU arch, no exec perms).
    child.on('error', (err) => reject(cliError(err, bin)));
    child.on('close', (code) => {
      try {
        const summary = parseSummary(JSON.parse(readFileSync(summaryPath, 'utf8')));
        let junit = '';
        try {
          junit = readFileSync(junitPath, 'utf8');
        } catch {
          /* a stopped run may not have flushed JUnit; summary still resolves */
        }
        resolve({ summary, junit });
      } catch {
        // No summary means the run failed before finishing — surface what loadr
        // printed (last lines) rather than a JSON-parse error.
        const detail = tail.trim().split(/\n/).slice(-6).join('\n').trim();
        reject(
          new Error(
            detail
              ? `The test run failed (exit ${code}):\n${detail}`
              : `The test run exited (code ${code}) without producing a summary.`,
          ),
        );
      }
    });
  });
}

function parseValidate(raw: string): ValidateResult {
  let diagnostics: Diagnostic[] = [];
  try {
    const parsed = JSON.parse(raw);
    diagnostics = Array.isArray(parsed) ? parsed : (parsed.diagnostics ?? []);
  } catch {
    /* leave diagnostics empty if the CLI printed non-JSON */
  }
  const ok = !diagnostics.some((d) => d.severity === 'error');
  return { ok, diagnostics, raw };
}
