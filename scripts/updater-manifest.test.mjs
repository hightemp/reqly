// node --test scripts/
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { buildManifest, updaterAssets } from './updater-manifest.mjs';

const base = 'https://github.com/hightemp/reqly/releases/download/v0.10.0';
const names = [
  'Reqly-0.10.0-1.x86_64.rpm',
  'Reqly_0.10.0_amd64.AppImage',
  'Reqly_0.10.0_amd64.AppImage.sig',
  'Reqly_0.10.0_amd64.deb',
  'Reqly_0.10.0_universal.dmg',
  'Reqly_universal.app.tar.gz',
  'Reqly_universal.app.tar.gz.sig',
  'Reqly_0.10.0_x64-setup.exe',
  'Reqly_0.10.0_x64-setup.exe.sig',
  'Reqly_0.10.0_x64_en-US.msi',
  'Reqly_0.10.0_x64_en-US.msi.sig',
  'reqly-cli-v0.10.0-linux-x86_64.tar.gz',
  'latest.json',
];
const assets = names.map((name) => ({ name, url: `${base}/${name}` }));

test('picks the signed updater bundles of each platform', () => {
  const found = updaterAssets(assets);
  assert.deepEqual(
    Object.fromEntries(Object.entries(found).map(([k, v]) => [k, v.name])),
    {
      'linux-x86_64-appimage': 'Reqly_0.10.0_amd64.AppImage',
      'darwin-aarch64-app': 'Reqly_universal.app.tar.gz',
      'darwin-x86_64-app': 'Reqly_universal.app.tar.gz',
      'windows-x86_64-nsis': 'Reqly_0.10.0_x64-setup.exe',
      'windows-x86_64-msi': 'Reqly_0.10.0_x64_en-US.msi',
    },
  );
  assert.equal(found['windows-x86_64-msi'].sig, 'Reqly_0.10.0_x64_en-US.msi.sig');
});

test('skips bundles without a signature', () => {
  const unsigned = assets.filter((a) => !a.name.endsWith('.msi.sig'));
  assert.equal(updaterAssets(unsigned)['windows-x86_64-msi'], undefined);
  assert.ok(updaterAssets(unsigned)['windows-x86_64-nsis']);
});

test('architecture-specific macOS bundles', () => {
  const mac = ['Reqly_aarch64.app.tar.gz', 'Reqly_aarch64.app.tar.gz.sig', 'Reqly_x64.app.tar.gz', 'Reqly_x64.app.tar.gz.sig'];
  const found = updaterAssets(mac.map((name) => ({ name, url: name })));
  assert.equal(found['darwin-aarch64-app'].name, 'Reqly_aarch64.app.tar.gz');
  assert.equal(found['darwin-x86_64-app'].name, 'Reqly_x64.app.tar.gz');
});

test('builds latest.json with signatures and default installers', () => {
  const signatures = Object.fromEntries(
    names.filter((n) => n.endsWith('.sig')).map((n) => [n, `sig of ${n}\n`]),
  );
  const manifest = buildManifest({
    tag: 'v0.10.0',
    notes: 'Notes',
    pubDate: '2026-11-04T10:00:00Z',
    assets,
    signatures,
  });
  assert.equal(manifest.version, '0.10.0');
  assert.equal(manifest.notes, 'Notes');
  assert.equal(manifest.pub_date, '2026-11-04T10:00:00Z');
  assert.deepEqual(manifest.platforms['linux-x86_64-appimage'], {
    url: `${base}/Reqly_0.10.0_amd64.AppImage`,
    signature: 'sig of Reqly_0.10.0_amd64.AppImage.sig',
  });
  // Plain `os-arch` keys for updaters that do not know their installer:
  // AppImage, the .app and NSIS.
  assert.equal(manifest.platforms['linux-x86_64'].url, `${base}/Reqly_0.10.0_amd64.AppImage`);
  assert.equal(manifest.platforms['darwin-aarch64'].url, `${base}/Reqly_universal.app.tar.gz`);
  assert.equal(manifest.platforms['windows-x86_64'].url, `${base}/Reqly_0.10.0_x64-setup.exe`);
  assert.equal(Object.keys(manifest.platforms).length, 9);
});

test('a release without signed bundles has no manifest', () => {
  assert.throws(
    () => buildManifest({ tag: 'v1.0.0', notes: '', pubDate: '', assets: [], signatures: {} }),
    /no signed updater bundles/,
  );
});

test('a signature that was not downloaded is an error', () => {
  assert.throws(
    () => buildManifest({ tag: 'v0.10.0', notes: '', pubDate: '', assets, signatures: {} }),
    /Reqly_0.10.0_amd64.AppImage.sig/,
  );
});
