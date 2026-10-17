// Builds the UI and starts reqly-server on a temporary data directory.
import { execSync, spawn } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const root = new URL('../..', import.meta.url).pathname;
execSync('pnpm build', { cwd: join(root, 'ui'), stdio: 'inherit' });
execSync('cargo build -p reqly-server', { cwd: root, stdio: 'inherit' });

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
