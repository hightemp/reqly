// Environment data (a JSON object) as editable rows and back.

export interface VarRow {
  id: string;
  name: string;
  /** Strings as typed; other JSON values as JSON text. */
  value: string;
  /** The value is JSON (number, boolean, object…) rather than a string. */
  json: boolean;
  secret: boolean;
}

export function toRows(
  data: Record<string, unknown>,
  secrets: string[],
  newId: () => string,
): VarRow[] {
  return Object.entries(data).map(([name, v]) => ({
    id: newId(),
    name,
    value: typeof v === 'string' ? v : JSON.stringify(v),
    json: typeof v !== 'string',
    secret: secrets.includes(name),
  }));
}

function parseValue(row: VarRow): unknown {
  if (!row.json) return row.value;
  try {
    return JSON.parse(row.value);
  } catch {
    return row.value;
  }
}

/** Rows to data; later rows win for duplicate names, empty names are skipped. */
export function fromRows(rows: VarRow[]): { data: Record<string, unknown>; secrets: string[] } {
  const data: Record<string, unknown> = {};
  const secrets = new Set<string>();
  for (const row of rows) {
    const name = row.name.trim();
    if (!name) continue;
    data[name] = parseValue(row);
    if (row.secret) secrets.add(name);
    else secrets.delete(name);
  }
  return { data, secrets: [...secrets].filter((s) => s in data) };
}

export type JsonParse = { ok: true; data: Record<string, unknown> } | { ok: false; error: string };

/** JSON mode: the text must be an object. */
export function parseJsonData(text: string): JsonParse {
  if (!text.trim()) return { ok: true, data: {} };
  try {
    const v: unknown = JSON.parse(text);
    if (!v || typeof v !== 'object' || Array.isArray(v)) return { ok: false, error: 'object' };
    return { ok: true, data: v as Record<string, unknown> };
  } catch (e) {
    return { ok: false, error: e instanceof Error ? e.message : String(e) };
  }
}

/** Valid variable names: what `{{ name }}` can reference without brackets. */
export const isPlainName = (name: string) => /^[\p{L}\w-]+$/u.test(name);
