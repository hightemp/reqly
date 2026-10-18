// Builds the UI and starts reqly-server on a temporary data directory.
import { execSync, spawn } from 'node:child_process';
import { existsSync, mkdtempSync, rmSync, statSync, writeFileSync } from 'node:fs';
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

const dataDir = mkdtempSync(join(tmpdir(), 'reqly-e2e-'));
const server = spawn(join(root, 'target/debug/reqly-server'), {
  stdio: 'inherit',
  env: { ...process.env, REQLY_DATA_DIR: dataDir },
});

const cleanup = () => {
  server.kill();
  rmSync(dataDir, { recursive: true, force: true });
};
process.on('SIGTERM', cleanup);
process.on('SIGINT', cleanup);
server.on('exit', (code) => process.exit(code ?? 0));
