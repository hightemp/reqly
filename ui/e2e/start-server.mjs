// Builds the UI and starts reqly-server on a temporary data directory.
import { execSync, spawn } from 'node:child_process';
import { existsSync, mkdtempSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:http';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const root = new URL('../..', import.meta.url).pathname;
// Written after each build, so the other servers never start a stale binary.
const marker = join(root, 'target/e2e-built');
const started = Date.now();
if (!process.argv.includes('--no-build')) {
  rmSync(marker, { force: true });
  execSync('pnpm build', { cwd: join(root, 'ui'), stdio: 'inherit' });
  execSync('cargo build -p reqly-server', { cwd: root, stdio: 'inherit' });
  writeFileSync(marker, String(Date.now()));
} else {
  // Wait for the first server process to finish building.
  const fresh = () => existsSync(marker) && statSync(marker).mtimeMs >= started - 5_000;
  for (let i = 0; i < 1200 && !fresh(); i++) await new Promise((r) => setTimeout(r, 500));
}

// Update checks: a fake release list with v99.0.0 (`--fake-releases`), or
// none at all, so tests never ask GitHub.
let releases = null;
const updateEnv = { REQLY_UPDATE_CHECK: 'false' };
if (process.argv.includes('--fake-releases')) {
  const body = JSON.stringify([
    {
      tag_name: 'v99.0.0',
      name: 'Reqly v99.0.0',
      body: '### Added\n- Everything',
      draft: false,
      prerelease: false,
      published_at: '2026-11-04T10:00:00Z',
      html_url: 'https://github.com/hightemp/reqly/releases/tag/v99.0.0',
      assets: [],
    },
  ]);
  releases = createServer((_, res) => {
    res.writeHead(200, { 'content-type': 'application/json' });
    res.end(body);
  });
  await new Promise((r) => releases.listen(0, '127.0.0.1', r));
  updateEnv.REQLY_UPDATE_CHECK = 'true';
  updateEnv.REQLY_UPDATE_URL = `http://127.0.0.1:${releases.address().port}/releases`;
}

const dataDir = mkdtempSync(join(tmpdir(), 'reqly-e2e-'));
const server = spawn(join(root, 'target/debug/reqly-server'), {
  stdio: 'inherit',
  env: { ...process.env, ...updateEnv, REQLY_DATA_DIR: dataDir },
});

const cleanup = () => {
  server.kill();
  releases?.close();
  rmSync(dataDir, { recursive: true, force: true });
};
process.on('SIGTERM', cleanup);
process.on('SIGINT', cleanup);
server.on('exit', (code) => process.exit(code ?? 0));
