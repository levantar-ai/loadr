// electron-builder runs this once per (platform, arch) before packing. It stages
// the loadr binary that MATCHES the arch being packed into resources/bin, so a
// macOS x64 DMG never ends up with an arm64 loadr (or vice versa) — the bug that
// surfaces at runtime as `spawn ... EBADARCH` (errno -86).
//
// macOS DMGs are built for both x64 and arm64 from one (arm64) runner, so CI
// cross-builds both Rust targets; this hook picks the right one per pack. When
// an arch-specific build is missing (e.g. a local single-arch `npm run package`)
// it falls back to the native binary stage-loadr.mjs placed, with a warning.

const { copyFileSync, existsSync, mkdirSync, chmodSync } = require('node:fs');
const { join } = require('node:path');

// electron-builder Arch enum → label.
const ARCH = { 0: 'ia32', 1: 'x64', 2: 'armv7l', 3: 'arm64', 4: 'universal' };

const MAC_TRIPLE = { x64: 'x86_64-apple-darwin', arm64: 'aarch64-apple-darwin' };

module.exports = async function beforePack(context) {
  const platform = context.electronPlatformName; // 'darwin' | 'win32' | 'linux'
  const arch = ARCH[context.arch] ?? String(context.arch);

  // Only macOS ships multiple arches from one build host; win/linux use the
  // single native binary already staged by stage-loadr.mjs.
  if (platform !== 'darwin') return;

  const repoRoot = join(__dirname, '..', '..');
  const destDir = join(__dirname, '..', 'resources', 'bin');
  const dest = join(destDir, 'loadr');

  const triple = MAC_TRIPLE[arch];
  const archSpecific = triple && join(repoRoot, 'target', triple, 'release', 'loadr');
  const native = join(repoRoot, 'target', 'release', 'loadr');

  let src = null;
  if (archSpecific && existsSync(archSpecific)) {
    src = archSpecific;
  } else if (existsSync(native)) {
    src = native;
    console.warn(
      `before-pack: WARNING no ${triple} build found; staging the native binary ` +
        `for the ${arch} package. Intel/ARM users may hit EBADARCH. ` +
        `Cross-build target/${triple}/release/loadr to fix.`,
    );
  } else if (existsSync(dest)) {
    console.warn(`before-pack: WARNING reusing already-staged binary for ${arch} (${dest}).`);
    return;
  }

  if (!src) {
    throw new Error(
      `before-pack: no loadr binary for darwin/${arch}. ` +
        `Build target/${triple}/release/loadr (or target/release/loadr).`,
    );
  }

  mkdirSync(destDir, { recursive: true });
  copyFileSync(src, dest);
  chmodSync(dest, 0o755);
  console.log(`before-pack: staged ${src} -> ${dest} (darwin/${arch})`);
};
