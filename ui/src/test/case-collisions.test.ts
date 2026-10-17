import { readdirSync, statSync } from 'node:fs';
import { join, relative } from 'node:path';
import { expect, it } from 'vitest';

// macOS and Windows file systems ignore case: `Foo.svelte` and
// `foo.svelte.ts` both resolve for an import of `./foo.svelte` there.
it('has no module paths that differ only in case', () => {
  const root = join(__dirname, '..');
  const seen = new Map<string, string>();
  const clashes: string[] = [];
  const walk = (dir: string) => {
    for (const name of readdirSync(dir)) {
      const path = join(dir, name);
      if (statSync(path).isDirectory()) {
        walk(path);
        continue;
      }
      const key = relative(root, path)
        .replace(/\.(svelte\.ts|svelte|ts)$/, '')
        .toLowerCase();
      const other = seen.get(key);
      if (other) clashes.push(`${other} <-> ${relative(root, path)}`);
      else seen.set(key, relative(root, path));
    }
  };
  walk(root);
  expect(clashes).toEqual([]);
});
