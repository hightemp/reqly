import { defineConfig, devices } from '@playwright/test';

const PORT = 8485;

// E2E runs against the real server binary with the built UI embedded and a
// throwaway data directory, so it covers HTTP + WebSocket transport too.
export default defineConfig({
  testDir: './e2e',
  fullyParallel: false,
  workers: 1,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 1 : 0,
  reporter: process.env.CI ? [['github'], ['html', { open: 'never' }]] : 'list',
  use: {
    baseURL: `http://127.0.0.1:${PORT}`,
    trace: 'retain-on-failure',
  },
  projects: [{ name: 'chromium', use: { ...devices['Desktop Chrome'], proxy: undefined } }],
  webServer: {
    command: 'node e2e/start-server.mjs',
    url: `http://127.0.0.1:${PORT}/healthz`,
    reuseExistingServer: false,
    timeout: 300_000,
    env: { REQLY_BIND: `127.0.0.1:${PORT}` },
  },
});
