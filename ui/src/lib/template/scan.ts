// Lightweight template scanner for highlighting, completion and the tag
// editor. The Rust parser is the source of truth for rendering; this only
// needs to find ranges and names.

export type TemplateKind = 'variable' | 'function' | 'tag';

export interface TemplateRange {
  kind: TemplateKind;
  from: number;
  to: number;
  /** Variable path (`api.v`) or function name (`uuid`). */
  name: string;
  /** First path segment of a variable, used to check it is defined. */
  root: string;
  text: string;
}

const PATH =
  /^\s*_\s*\[\s*(['"])(.*?)\1\s*\]|^\s*(?:_\s*\.\s*)?([\w-]+(?:\s*(?:\.\s*[\w-]+|\[[^\]]*\]))*)/;

function variableName(inner: string): { name: string; root: string } | null {
  const m = PATH.exec(inner);
  if (!m) return null;
  if (m[2] !== undefined) return { name: m[2], root: m[2] };
  const name = m[3]!.replace(/\s+/g, '');
  const root = /^[\w-]+/.exec(name)![0];
  return { name, root };
}

/** Index of `close` after `from`, skipping quoted strings and `{…}`, `[…]`, `(…)` arguments. */
function findClose(src: string, from: number, close: string): number {
  let depth = 0;
  for (let j = from; j < src.length; j++) {
    const c = src[j]!;
    if (depth === 0 && src.startsWith(close, j)) return j;
    if (c === "'" || c === '"') {
      const q = c;
      for (j++; j < src.length && src[j] !== q; j++) if (src[j] === '\\') j++;
    } else if (c === '{' || c === '[' || c === '(') depth++;
    else if ((c === '}' || c === ']' || c === ')') && depth > 0) depth--;
  }
  return -1;
}

export function scanTemplates(src: string): TemplateRange[] {
  const out: TemplateRange[] = [];
  let i = 0;
  while (i < src.length) {
    const open = src.indexOf('{', i);
    if (open < 0 || open + 1 >= src.length) break;
    if (open > 0 && src[open - 1] === '\\') {
      i = open + 2;
      continue;
    }
    const next = src[open + 1];
    const close = next === '{' ? '}}' : next === '%' ? '%}' : next === '#' ? '#}' : null;
    if (!close) {
      i = open + 1;
      continue;
    }
    const end = next === '#' ? src.indexOf(close, open + 2) : findClose(src, open + 2, close);
    if (end < 0) break;
    const to = end + 2;
    const inner = src.slice(open + 2, end);
    const text = src.slice(open, to);
    if (next === '%') {
      const name = /^\s*([\w-]+)/.exec(inner)?.[1];
      if (name) out.push({ kind: 'tag', from: open, to, name, root: name, text });
    } else if (next === '{') {
      const call = /^\s*\$([\w-]+)/.exec(inner);
      if (call) {
        out.push({ kind: 'function', from: open, to, name: call[1]!, root: call[1]!, text });
      } else {
        const v = variableName(inner);
        if (v) out.push({ kind: 'variable', from: open, to, ...v, text });
      }
    }
    i = to;
  }
  return out;
}

// ----- tag arguments ----------------------------------------------------------

export type ArgKind = 'string' | 'number' | 'boolean' | 'variable';

export interface TagArg {
  kind: ArgKind;
  /** Unquoted string / number literal / variable path. */
  value: string;
}

export interface ParsedTag {
  name: string;
  args: TagArg[];
}

function readArgs(s: string): TagArg[] | null {
  const args: TagArg[] = [];
  let i = 0;
  const ws = () => {
    while (i < s.length && /\s/.test(s[i]!)) i++;
  };
  ws();
  if (i >= s.length) return args;
  for (;;) {
    ws();
    const c = s[i];
    if (c === "'" || c === '"') {
      let value = '';
      i++;
      while (i < s.length && s[i] !== c) {
        if (s[i] === '\\' && i + 1 < s.length) i++;
        value += s[i];
        i++;
      }
      if (s[i] !== c) return null;
      i++;
      args.push({ kind: 'string', value });
    } else if (c === '{' || c === '[') {
      // Object and array arguments are edited as text.
      return null;
    } else {
      const m = /^[^,]+/.exec(s.slice(i));
      if (!m) return null;
      const raw = m[0].trim();
      i += m[0].length;
      if (/^-?\d+(\.\d+)?([eE][-+]?\d+)?$/.test(raw)) args.push({ kind: 'number', value: raw });
      else if (raw === 'true' || raw === 'false') args.push({ kind: 'boolean', value: raw });
      else args.push({ kind: 'variable', value: raw.replace(/^_\./, '') });
    }
    ws();
    if (i >= s.length) return args;
    if (s[i] !== ',') return null;
    i++;
  }
}

/** Parses `{% name a, b %}` or `{{ $name(a, b) }}`; `null` when it cannot. */
export function parseTag(text: string): ParsedTag | null {
  const tag = /^\{%\s*([\w-]+)\s*([\s\S]*?)\s*%\}$/.exec(text);
  if (tag) {
    const args = readArgs(tag[2]!);
    return args ? { name: tag[1]!, args } : null;
  }
  const call = /^\{\{\s*\$([\w-]+)\s*(?:\(([\s\S]*)\))?\s*\}\}$/.exec(text);
  if (call) {
    const args = readArgs(call[2] ?? '');
    return args ? { name: call[1]!, args } : null;
  }
  return null;
}

function argText(a: TagArg): string {
  if (a.kind === 'string') return `'${a.value.replace(/\\/g, '\\\\').replace(/'/g, "\\'")}'`;
  return a.value || "''";
}

export function serializeTag(tag: ParsedTag): string {
  const args = tag.args.map(argText).join(', ');
  return args ? `{% ${tag.name} ${args} %}` : `{% ${tag.name} %}`;
}

// ----- completion ---------------------------------------------------------------

export interface CompletionAt {
  kind: 'variable' | 'tag' | 'function' | 'faker';
  prefix: string;
  /** Offset where the completed word starts. */
  from: number;
}

/** What to complete at the end of `before` (the text left of the cursor). */
export function completionContext(before: string): CompletionAt | null {
  const open = Math.max(before.lastIndexOf('{{'), before.lastIndexOf('{%'));
  if (open < 0) return null;
  const inner = before.slice(open + 2);
  if (/(\}\}|%\})/.test(inner)) return null;
  const isTag = before[open + 1] === '%';
  const lead = /^\s*/.exec(inner)![0].length;
  const start = open + 2 + lead;
  const word = inner.slice(lead);
  if (isTag) {
    return /^[\w-]*$/.test(word) ? { kind: 'tag', prefix: word, from: start } : null;
  }
  const faker = /^\$faker\.([\w.]*)$/.exec(word);
  if (faker) return { kind: 'faker', prefix: faker[1]!, from: start + 7 };
  const fn = /^\$([\w-]*)$/.exec(word);
  if (fn) return { kind: 'function', prefix: fn[1]!, from: start + 1 };
  if (/^(_\.)?[\w.-]*$/.test(word)) return { kind: 'variable', prefix: word, from: start };
  return null;
}
