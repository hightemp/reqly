// Builds the UI and starts reqly-server on a temporary data directory.
import { execSync, spawn } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const root = new URL('../..', import.meta.url).pathname;
if (!process.argv.includes('--no-build')) {
  execSync('pnpm build', { cwd: join(root, 'ui'), stdio: 'inherit' });
  execSync('cargo build -p reqly-server', { cwd: root, stdio: 'inherit' });
} else {
  // Wait for the other server process to finish building.
  const { existsSync } = await import('node:fs');
  const bin = join(root, 'target/debug/reqly-server');
  for (let i = 0; i < 600 && !existsSync(bin); i++) await new Promise((r) => setTimeout(r, 500));
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
