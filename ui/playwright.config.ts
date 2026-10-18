import { defineConfig, devices } from '@playwright/test';

const PORT = 8485;
export const TOKEN_PORT = 8487;
export const E2E_TOKEN = 'e2e-token-1234567890';
export const MULTI_PORT = 8488;
export const E2E_ADMIN = { username: 'admin', password: 'admin-password' };

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
  webServer: [
    {
      command: 'node e2e/start-server.mjs',
      url: `http://127.0.0.1:${PORT}/healthz`,
      reuseExistingServer: false,
      timeout: 300_000,
      env: { REQLY_BIND: `127.0.0.1:${PORT}` },
    },
    {
      // Same binary in token-auth mode (built by the first server's command).
      command: 'node e2e/start-server.mjs --no-build',
      url: `http://127.0.0.1:${TOKEN_PORT}/healthz`,
      reuseExistingServer: false,
      timeout: 300_000,
      env: {
        REQLY_BIND: `127.0.0.1:${TOKEN_PORT}`,
        REQLY_AUTH_MODE: 'token',
        REQLY_AUTH_TOKEN: E2E_TOKEN,
      },
    },
    {
      // And in multi-user mode with open registration.
      command: 'node e2e/start-server.mjs --no-build',
      url: `http://127.0.0.1:${MULTI_PORT}/healthz`,
      reuseExistingServer: false,
      timeout: 300_000,
      env: {
        REQLY_BIND: `127.0.0.1:${MULTI_PORT}`,
        REQLY_AUTH_MODE: 'multi',
        REQLY_REGISTRATION: 'open',
        REQLY_ADMIN_USERNAME: E2E_ADMIN.username,
        REQLY_ADMIN_PASSWORD: E2E_ADMIN.password,
      },
    },
  ],
});
