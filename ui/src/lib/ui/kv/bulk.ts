import type { KeyValue } from '$lib/api/generated/KeyValue';

export type KvRow = KeyValue;

/** Text form: one `name: value` per line, disabled rows prefixed with `// `. */
export function toBulk(rows: KvRow[], separator = ':'): string {
  const sep = separator === ':' ? ': ' : separator;
  return rows
    .filter((r) => r.name || r.value)
    .map((r) => `${r.enabled ? '' : '// '}${r.name}${sep}${r.value}`)
    .join('\n');
}

export function fromBulk(
  text: string,
  existing: KvRow[],
  newId: () => string,
  separator = ':',
): KvRow[] {
  const pool = [...existing];
  const rows: KvRow[] = [];
  for (const raw of text.split('\n')) {
    let line = raw.trim();
    if (!line) continue;
    const enabled = !line.startsWith('//');
    if (!enabled) line = line.replace(/^\/\/\s?/, '');
    const at = line.indexOf(separator);
    const name = (at < 0 ? line : line.slice(0, at)).trim();
    const value = at < 0 ? '' : line.slice(at + separator.length).trim();
    const reuse = pool.findIndex((r) => r.name === name);
    const base = reuse >= 0 ? pool.splice(reuse, 1)[0]! : undefined;
    rows.push({
      id: base?.id ?? newId(),
      enabled,
      name,
      value,
      description: base?.description ?? '',
      kind: base?.kind ?? 'text',
    });
  }
  return rows;
}
