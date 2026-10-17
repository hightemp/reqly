import { fileURLToPath } from 'node:url';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { svelteTesting } from '@testing-library/svelte/vite';
import { defineConfig } from 'vitest/config';

const src = (p: string) => fileURLToPath(new URL(`./src/${p}`, import.meta.url));

export default defineConfig({
  plugins: [svelte(), svelteTesting()],
  resolve: {
    alias: { $lib: src('lib'), $features: src('features') },
  },
  // Tauri expects a fixed port and must not have the screen cleared.
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
    proxy: {
      '/api': { target: 'http://127.0.0.1:8484', ws: true },
      '/healthz': 'http://127.0.0.1:8484',
    },
  },
  build: {
    target: 'es2022',
    sourcemap: true,
  },
  test: {
    environment: 'jsdom',
    globals: true,
    include: ['src/**/*.test.ts'],
    setupFiles: ['src/test/setup.ts'],
    coverage: {
      provider: 'v8',
      include: ['src/lib/**/*.{ts,svelte}'],
      exclude: ['src/lib/api/generated/**', 'src/**/*.test.ts'],
      thresholds: { lines: 80 },
    },
  },
});
