import { defineConfig } from '@playwright/test';

// Smoke test of the Android app on an emulator or device (adb). The APK must
// be installed first; see docs/android.md.
export default defineConfig({
  testDir: './e2e-android',
  fullyParallel: false,
  workers: 1,
  timeout: 120_000,
  retries: process.env.CI ? 1 : 0,
  reporter: process.env.CI ? [['github'], ['list']] : 'list',
});
