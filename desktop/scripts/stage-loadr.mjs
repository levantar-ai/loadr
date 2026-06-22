// Stage a loadr binary into resources/bin before packaging, so the app ships
// with the CLI it drives. Run as part of `npm run package`.
//
// On macOS the build cross-compiles both arches (x86_64/aarch64-apple-darwin)
// and scripts/before-pack.cjs swaps in the arch-matching binary per pack; this
// script just needs to place SOMETHING so electron-builder's extraResources copy
// has a file. Source order: $LOADR_BIN, the native release build, then (mac) the
// first arch-specific build that exists.
import { copyFileSync, existsSync, mkdirSync } from 'node:fs';
import { join } from 'node:path';

const exe = process.platform === 'win32' ? 'loadr.exe' : 'loadr';

const candidates = [process.env.LOADR_BIN, join('..', 'target', 'release', exe)];
if (process.platform === 'darwin') {
  candidates.push(
    join('..', 'target', 'aarch64-apple-darwin', 'release', exe),
    join('..', 'target', 'x86_64-apple-darwin', 'release', exe),
  );
}

const src = candidates.find((p) => p && existsSync(p));
if (!src) {
  console.error(
    `stage-loadr: no loadr binary found (looked in: ${candidates.filter(Boolean).join(', ')})\n` +
      'Build it (cargo build --release -p loadr-cli) or set $LOADR_BIN.',
  );
  process.exit(1);
}

const destDir = join('resources', 'bin');
mkdirSync(destDir, { recursive: true });
const dest = join(destDir, exe);
copyFileSync(src, dest);
console.log(`stage-loadr: ${src} -> ${dest}`);
