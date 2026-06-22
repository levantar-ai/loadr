import { describe, expect, it } from 'vitest';

import { cleanIpcMessage } from './ipc';

describe('cleanIpcMessage', () => {
  it('strips the Electron "invoking remote method" wrapper and Error prefixes', () => {
    const raw = "Error invoking remote method 'plan:run': Error: The bundled loadr engine doesn't match this Mac's processor.";
    expect(cleanIpcMessage(raw)).toBe("The bundled loadr engine doesn't match this Mac's processor.");
  });

  it('handles multiple stacked Error prefixes', () => {
    const raw = "Error invoking remote method 'loadr:schema': Error: Error: boom";
    expect(cleanIpcMessage(raw)).toBe('boom');
  });

  it('preserves multi-line bodies after the wrapper', () => {
    const raw = "Error invoking remote method 'plan:run': Error: The test run failed (exit 1):\nerror: bad plan";
    expect(cleanIpcMessage(raw)).toBe('The test run failed (exit 1):\nerror: bad plan');
  });

  it('returns plain messages unchanged', () => {
    expect(cleanIpcMessage('just a message')).toBe('just a message');
  });

  it('never returns empty for an empty input', () => {
    expect(cleanIpcMessage('')).toBe('Something went wrong.');
  });
});
