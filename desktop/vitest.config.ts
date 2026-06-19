import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    // Default to node (the round-trip core uses fs/child_process). Component
    // tests added in later milestones opt into jsdom per-file.
    environment: 'node',
    include: ['src/**/*.test.ts', 'src/**/*.test.tsx'],
  },
});
