// Parse `loadr plugin list` into structured rows for the plugins panel. When the
// CLI is spawned non-interactively its colour output is auto-disabled, so the
// table is plain text. Pure → headless-testable; the spawn lives in main.

export interface InstalledPlugin {
  name: string;
  kind: string;
  type: string;
  state: 'enabled' | 'disabled';
  version: string;
}

// Defensive strip of an ANSI SGR sequence's printable tail (e.g. "[0m", "[32m").
const ANSI = /\[[0-9;]*m/g;

export function parsePluginList(stdout: string): InstalledPlugin[] {
  return stdout
    .replace(ANSI, '')
    .split('\n')
    .map((l) => l.trimEnd())
    .filter(Boolean)
    .filter((l) => !/^NAME\s+KIND/i.test(l) && !/^no plugins/i.test(l))
    .map((l) => l.split(/\s+/))
    .filter((parts) => parts.length >= 5)
    .map(([name, kind, type, state, version]) => ({
      name,
      kind,
      type,
      state: state === 'disabled' ? ('disabled' as const) : ('enabled' as const),
      version,
    }));
}
