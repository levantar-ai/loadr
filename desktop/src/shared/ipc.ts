// Pure helper shared by preload and tests. Electron wraps a rejected IPC handler
// as "Error invoking remote method 'channel': Error: <real message>", which is
// noise to a user. Strip the wrapper so the renderer shows just our message.

export function cleanIpcMessage(message: string): string {
  if (!message) return 'Something went wrong.';
  const m = message.match(/Error invoking remote method '[^']*':\s*(.*)/s);
  let out = m ? m[1] : message;
  // Strip one or more leading "Error: " prefixes Electron/Node stack into the text.
  out = out.replace(/^(?:[A-Za-z]*Error:\s*)+/, '');
  return out.trim() || message;
}
