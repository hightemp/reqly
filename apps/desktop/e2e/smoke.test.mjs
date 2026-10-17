// Desktop smoke test through tauri-driver (WebDriver). Linux CI only:
// requires `tauri-driver` (cargo install tauri-driver) and WebKitWebDriver
// (apt: webkit2gtk-driver). Run after `cargo build -p reqly-desktop --release`.
import { spawn } from 'node:child_process';
import { mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { after, before, test } from 'node:test';
import assert from 'node:assert/strict';
import { remote } from 'webdriverio';

const root = new URL('../../..', import.meta.url).pathname;
const application = join(root, 'target/release/reqly-desktop');
let driver;
let browser;

before(async () => {
  driver = spawn('tauri-driver', [], {
    stdio: 'inherit',
    // Isolated app data dir so the test starts from a clean database.
    env: { ...process.env, XDG_DATA_HOME: mkdtempSync(join(tmpdir(), 'reqly-desktop-e2e-')) },
  });
  await new Promise((r) => setTimeout(r, 1000));
  browser = await remote({
    hostname: '127.0.0.1',
    port: 4444,
    capabilities: { 'tauri:options': { application } },
    logLevel: 'warn',
  });
});

after(async () => {
  await browser?.deleteSession();
  driver?.kill();
});

test('shows the app shell', async () => {
  const sidebar = await browser.$('aside[aria-label]');
  await sidebar.waitForDisplayed({ timeout: 15000 });
  assert.equal(await sidebar.getAttribute('aria-label'), 'Collections');
});

test('custom titlebar has window controls', async () => {
  assert.ok(await browser.$('button[aria-label="Close"]').isDisplayed());
});
