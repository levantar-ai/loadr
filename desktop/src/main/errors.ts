// Turn raw child-process spawn failures into messages a human can act on.
//
// The motivating case: a macOS user whose bundled `loadr` is the wrong CPU type
// gets `spawn Unknown system error -86` (EBADARCH) — Node reports it as errno
// -86 / code 'UNKNOWN', so it can't be matched by code alone. We map the common
// failure modes to plain English with a concrete next step. Pure + testable.

export interface SpawnErrorLike {
  code?: string;
  errno?: number;
  syscall?: string;
  message?: string;
}

/** A spawn-class error means the binary couldn't even start (vs. ran and failed). */
export function isSpawnError(err: SpawnErrorLike): boolean {
  return typeof err?.syscall === 'string' && err.syscall.startsWith('spawn');
}

function archLabel(arch: string): string {
  if (arch === 'arm64') return 'Apple Silicon (arm64)';
  if (arch === 'x64') return 'Intel (x86_64)';
  return arch;
}

/**
 * Describe why the loadr engine couldn't be started.
 * `arch`/`platform` default to the running process so callers rarely pass them;
 * tests pass them explicitly.
 */
export function describeSpawnError(
  err: SpawnErrorLike,
  binPath: string,
  platform: NodeJS.Platform = process.platform,
  arch: string = process.arch,
): string {
  const { code, errno } = err;

  // macOS: wrong CPU type. errno -86 = EBADARCH; Node surfaces code 'UNKNOWN'.
  if (platform === 'darwin' && (errno === -86 || code === 'EBADARCH')) {
    return (
      `The bundled loadr engine doesn't match this Mac's processor (${archLabel(arch)}). ` +
      `This download was built for the other architecture. ` +
      `Download the ${arch === 'arm64' ? 'Apple Silicon (arm64)' : 'Intel (x64)'} build of loadr Desktop ` +
      `from https://loadr.io/download and reinstall.`
    );
  }

  // Linux/other: exec format error (wrong arch or corrupt). errno -8 = ENOEXEC.
  if (errno === -8 || code === 'ENOEXEC') {
    return (
      `The bundled loadr engine at ${binPath} isn't a runnable executable on this system ` +
      `(wrong architecture or a corrupt download). Reinstall the loadr Desktop build for your OS and CPU.`
    );
  }

  if (code === 'ENOENT') {
    return (
      `Couldn't find the loadr engine at ${binPath}. ` +
      `The app may be incomplete — reinstall loadr Desktop, or set the LOADR_BIN environment variable ` +
      `to a loadr binary.`
    );
  }

  if (code === 'EACCES' || code === 'EPERM') {
    return (
      `The loadr engine at ${binPath} isn't executable (permission denied). ` +
      `Reinstall loadr Desktop${platform !== 'win32' ? `, or run: chmod +x "${binPath}"` : ''}.`
    );
  }

  const detail = code && code !== 'UNKNOWN' ? code : errno != null ? `errno ${errno}` : 'unknown error';
  return `Couldn't start the loadr engine (${detail}) at ${binPath}.`;
}

/**
 * Wrap any error from invoking the CLI into an Error with a friendly message.
 * Spawn failures get `describeSpawnError`; a binary that ran but exited non-zero
 * keeps its stderr (trimmed) so real CLI diagnostics still reach the user.
 */
export function cliError(err: unknown, binPath: string): Error {
  const e = (err ?? {}) as SpawnErrorLike & { stderr?: string };
  if (isSpawnError(e)) {
    return new Error(describeSpawnError(e, binPath));
  }
  const stderr = typeof e.stderr === 'string' ? e.stderr.trim() : '';
  if (stderr) return new Error(stderr);
  return err instanceof Error ? err : new Error(String(err));
}
