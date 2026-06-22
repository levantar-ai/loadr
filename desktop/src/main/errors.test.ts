import { describe, expect, it } from 'vitest';

import { cliError, describeSpawnError, isSpawnError } from './errors';

const BIN = '/Applications/loadr Desktop.app/Contents/Resources/bin/loadr';

describe('describeSpawnError', () => {
  it('explains macOS EBADARCH (errno -86, code UNKNOWN) with an arch-specific fix', () => {
    const msg = describeSpawnError({ errno: -86, code: 'UNKNOWN', syscall: `spawn ${BIN}` }, BIN, 'darwin', 'arm64');
    expect(msg).toContain("doesn't match this Mac's processor");
    expect(msg).toContain('Apple Silicon (arm64)');
    expect(msg).toContain('loadr.io/download');
    expect(msg).not.toContain('-86');
  });

  it('points Intel Macs at the x64 build', () => {
    const msg = describeSpawnError({ errno: -86, syscall: 'spawn loadr' }, BIN, 'darwin', 'x64');
    expect(msg).toContain('Intel (x86_64)');
    expect(msg).toContain('Intel (x64) build');
  });

  it('explains ENOENT (missing binary)', () => {
    const msg = describeSpawnError({ code: 'ENOENT', syscall: 'spawn loadr' }, BIN, 'linux', 'x64');
    expect(msg).toContain("Couldn't find the loadr engine");
    expect(msg).toContain(BIN);
    expect(msg).toContain('LOADR_BIN');
  });

  it('explains EACCES (not executable) with chmod on unix only', () => {
    const unix = describeSpawnError({ code: 'EACCES', syscall: 'spawn loadr' }, BIN, 'linux', 'x64');
    expect(unix).toContain('permission denied');
    expect(unix).toContain('chmod +x');
    const win = describeSpawnError({ code: 'EACCES', syscall: 'spawn loadr' }, 'C:/loadr.exe', 'win32', 'x64');
    expect(win).not.toContain('chmod');
  });

  it('explains exec-format errors (ENOEXEC / errno -8)', () => {
    const msg = describeSpawnError({ errno: -8, code: 'UNKNOWN', syscall: 'spawn loadr' }, BIN, 'linux', 'x64');
    expect(msg).toContain('wrong architecture or a corrupt download');
  });

  it('falls back to a concrete code/errno for unknown failures', () => {
    expect(describeSpawnError({ code: 'EMFILE', syscall: 'spawn loadr' }, BIN, 'linux', 'x64')).toContain('EMFILE');
    expect(describeSpawnError({ errno: -123, code: 'UNKNOWN', syscall: 'spawn loadr' }, BIN, 'linux', 'x64')).toContain('errno -123');
  });
});

describe('isSpawnError', () => {
  it('is true only for spawn syscalls', () => {
    expect(isSpawnError({ syscall: 'spawn loadr', code: 'ENOENT' })).toBe(true);
    expect(isSpawnError({ code: 'ENOENT' })).toBe(false);
    expect(isSpawnError({ syscall: 'open', code: 'ENOENT' })).toBe(false);
  });
});

describe('cliError', () => {
  it('maps spawn failures through describeSpawnError (platform-agnostic case)', () => {
    const err = cliError({ code: 'ENOENT', syscall: `spawn ${BIN}` }, BIN);
    expect(err.message).toContain("Couldn't find the loadr engine");
    expect(err.message).toContain(BIN);
  });

  it('surfaces stderr when the binary ran but exited non-zero', () => {
    const err = cliError({ code: 1, stderr: 'error: bad flag\n', stdout: '' }, BIN);
    expect(err.message).toBe('error: bad flag');
  });

  it('passes through a plain Error untouched', () => {
    expect(cliError(new Error('boom'), BIN).message).toBe('boom');
  });
});
