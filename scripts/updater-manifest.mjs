#!/usr/bin/env node
// Builds the Tauri updater manifest (latest.json) of a release from its
// signed bundles and uploads it: node scripts/updater-manifest.mjs v0.10.0
// Needs `gh` signed in; `--dry-run` prints the manifest instead.
import { execFileSync } from 'node:child_process';
import { mkdtempSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

/** Updater bundles by asset name: [pattern, platform keys]. */
const BUNDLES = [
  [/_amd64\.AppImage$/, ['linux-x86_64-appimage']],
  [/_universal\.app\.tar\.gz$/, ['darwin-aarch64-app', 'darwin-x86_64-app']],
  [/_aarch64\.app\.tar\.gz$/, ['darwin-aarch64-app']],
  [/_x64\.app\.tar\.gz$/, ['darwin-x86_64-app']],
  [/_x64-setup\.exe$/, ['windows-x86_64-nsis']],
  [/_x64_[^_]+\.msi$/, ['windows-x86_64-msi']],
];

/** The installer an updater gets when it does not know its own. */
const DEFAULTS = {
  'linux-x86_64': 'linux-x86_64-appimage',
  'darwin-aarch64': 'darwin-aarch64-app',
  'darwin-x86_64': 'darwin-x86_64-app',
  'windows-x86_64': 'windows-x86_64-nsis',
};

/** Signed updater bundles by platform key: `{ name, url, sig }`. */
export function updaterAssets(assets) {
  const names = new Set(assets.map((a) => a.name));
  const found = {};
  for (const asset of assets) {
    const sig = `${asset.name}.sig`;
    if (!names.has(sig)) continue;
    const bundle = BUNDLES.find(([pattern]) => pattern.test(asset.name));
    // An architecture-specific bundle wins over the universal one.
    for (const key of bundle?.[1] ?? []) {
      if (!found[key] || /_universal\./.test(found[key].name)) {
        found[key] = { name: asset.name, url: asset.url, sig };
      }
    }
  }
  return found;
}

/** latest.json; `signatures` maps `.sig` asset names to their contents. */
export function buildManifest({ tag, notes, pubDate, assets, signatures }) {
  const found = updaterAssets(assets);
  if (Object.keys(found).length === 0) {
    throw new Error(`${tag} has no signed updater bundles`);
  }
  const platforms = {};
  for (const [key, bundle] of Object.entries(found)) {
    const signature = signatures[bundle.sig];
    if (signature === undefined) throw new Error(`missing signature ${bundle.sig}`);
    platforms[key] = { url: bundle.url, signature: signature.trim() };
  }
  for (const [key, installer] of Object.entries(DEFAULTS)) {
    if (platforms[installer]) platforms[key] = platforms[installer];
  }
  return { version: tag.replace(/^v/, ''), notes, pub_date: pubDate, platforms };
}

function gh(...args) {
  return execFileSync('gh', args, { encoding: 'utf8', stdio: ['ignore', 'pipe', 'inherit'] });
}

function main([tag, flag]) {
  if (!tag) {
    console.error('usage: updater-manifest.mjs <tag> [--dry-run]');
    process.exit(2);
  }
  const release = JSON.parse(gh('release', 'view', tag, '--json', 'assets,body,publishedAt'));
  const dir = mkdtempSync(join(tmpdir(), 'reqly-sigs-'));
  gh('release', 'download', tag, '--pattern', '*.sig', '--dir', dir, '--clobber');
  const signatures = Object.fromEntries(
    readdirSync(dir).map((name) => [name, readFileSync(join(dir, name), 'utf8')]),
  );
  const manifest = buildManifest({
    tag,
    notes: release.body ?? '',
    pubDate: release.publishedAt || new Date().toISOString(),
    assets: release.assets,
    signatures,
  });
  const text = `${JSON.stringify(manifest, null, 2)}\n`;
  if (flag === '--dry-run') {
    process.stdout.write(text);
    return;
  }
  const file = join(dir, 'latest.json');
  writeFileSync(file, text);
  gh('release', 'upload', tag, file, '--clobber');
  console.log(`latest.json: ${Object.keys(manifest.platforms).join(', ')}`);
}

if (process.argv[1] === fileURLToPath(import.meta.url)) main(process.argv.slice(2));
