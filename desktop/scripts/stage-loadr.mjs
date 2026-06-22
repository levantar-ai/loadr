// Stage a loadr binary into resources/bin before packaging, so the app ships
// with the CLI it drives. Run as part of `npm run package`.
//
// On macOS we bundle a UNIVERSAL2 (fat) loadr — one binary carrying both x86_64
// and arm64 slices — so a single bundled file runs natively on Intel AND Apple
// Silicon (and under Rosetta). This is what avoids the `spawn ... EBADARCH`
// (errno -86) failure, and unlike a per-arch swap it can't race electron-builder
// packing both arches concurrently. CI builds both targets and `lipo`s them into
// target/universal-apple-darwin/release/loadr (see the workflows). Source order:
// $LOADR_BIN, the universal build (mac), the native release build, then (mac) a
// single arch-specific build as a last resort.
import { copyFileSync, existsSync, mkdirSync } from 'node:fs';
import { join } from 'node:path';

const exe = process.platform === 'win32' ? 'loadr.exe' : 'loadr';

const candidates = [process.env.LOADR_BIN];
if (process.platform === 'darwin') {
  candidates.push(join('..', 'target', 'universal-apple-darwin', 'release', exe));
}
candidates.push(join('..', 'target', 'release', exe));
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
