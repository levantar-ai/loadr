import { resolve } from 'node:path';

import react from '@vitejs/plugin-react';
import tailwindcss from '@tailwindcss/vite';
import { defineConfig } from 'electron-vite';

export default defineConfig({
  main: {
    build: {
      lib: { entry: resolve(__dirname, 'src/main/index.ts') },
      rollupOptions: { output: { dir: 'out/main' } },
    },
  },
  preload: {
    build: {
      lib: { entry: resolve(__dirname, 'src/preload/index.ts') },
      rollupOptions: { output: { dir: 'out/preload' } },
    },
  },
  renderer: {
    root: 'src/renderer',
    plugins: [react(), tailwindcss()],
    build: {
      rollupOptions: { input: resolve(__dirname, 'src/renderer/index.html') },
      outDir: 'out/renderer',
    },
  },
});
