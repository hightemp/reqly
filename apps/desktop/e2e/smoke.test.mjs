// Desktop smoke test through tauri-driver (WebDriver). Linux CI only:
// requires `tauri-driver` (cargo install tauri-driver) and WebKitWebDriver
// (apt: webkit2gtk-driver). Run after `cargo build -p reqly-desktop --release`.
import { spawn } from 'node:child_process';
import { mkdtempSync } from 'node:fs';
import { createServer } from 'node:http';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { after, before, test } from 'node:test';
import assert from 'node:assert/strict';
import { remote } from 'webdriverio';

const root = new URL('../../..', import.meta.url).pathname;
const application = join(root, 'target/release/reqly-desktop');
let driver;
let browser;
let releases;

/** A fake GitHub release list with a newer version (REQLY_UPDATE_URL). */
async function startReleases() {
  const body = JSON.stringify([
    {
      tag_name: 'v99.0.0',
      name: 'Reqly v99.0.0',
      body: 'Big update',
      draft: false,
      prerelease: false,
      published_at: '2026-11-04T10:00:00Z',
      html_url: 'https://github.com/hightemp/reqly/releases/tag/v99.0.0',
      assets: [],
    },
  ]);
  const server = createServer((_, res) => {
    res.writeHead(200, { 'content-type': 'application/json' });
    res.end(body);
  });
  await new Promise((r) => server.listen(0, '127.0.0.1', r));
  return server;
}

before(async () => {
  releases = await startReleases();
  driver = spawn('tauri-driver', [], {
    stdio: 'inherit',
    env: {
      ...process.env,
      // Isolated app data dir so the test starts from a clean database.
      XDG_DATA_HOME: mkdtempSync(join(tmpdir(), 'reqly-desktop-e2e-')),
      REQLY_UPDATE_URL: `http://127.0.0.1:${releases.address().port}/releases`,
      NO_PROXY: '*',
      no_proxy: '*',
    },
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
  releases?.close();
});

test('shows the app shell', async () => {
  const sidebar = await browser.$('aside[aria-label]');
  await sidebar.waitForDisplayed({ timeout: 15000 });
  assert.equal(await sidebar.getAttribute('aria-label'), 'Collections');
});

test('custom titlebar has window controls', async () => {
  assert.ok(await browser.$('button[aria-label="Close"]').isDisplayed());
});

test('offers a newer release', async () => {
  const status = await browser.$('button=Update 99.0.0');
  await status.waitForDisplayed({ timeout: 15000 });
  await status.click();
  const lead = await browser.$('p=Reqly 99.0.0 is available. You have ' + (await version()) + '.');
  await lead.waitForDisplayed({ timeout: 5000 });
  // A release build without the updater key (or not an AppImage) links to the download.
  assert.ok(await browser.$('button=Download').isDisplayed());
  await browser.$('button=Later').click();
});

async function version() {
  const { readFileSync } = await import('node:fs');
  return readFileSync(join(root, 'Cargo.toml'), 'utf8').match(/^version = "(.*)"/m)[1];
}
