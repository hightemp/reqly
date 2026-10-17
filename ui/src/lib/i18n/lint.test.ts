// i18n lint, run as part of the unit test suite (PRD §7.19):
//  * no hard-coded user-visible text in .svelte components;
//  * every key in `en` is used somewhere (or covered by an `i18n-dynamic:` prefix).
import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join, relative } from 'node:path';
import { parse } from 'svelte/compiler';
import { describe, expect, it } from 'vitest';
import { en } from './locales/en';

const SRC = join(__dirname, '..', '..');
const LETTER = /\p{L}/u;
const TEXT_ATTRIBUTES = new Set(['title', 'placeholder', 'aria-label', 'alt', 'aria-description']);

function files(dir: string, ext: string[]): string[] {
  return readdirSync(dir).flatMap((name) => {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) return name === 'generated' ? [] : files(path, ext);
    return ext.some((e) => path.endsWith(e)) ? [path] : [];
  });
}

type Node = {
  type: string;
  data?: string;
  name?: string;
  value?: unknown;
  start: number;
  [k: string]: unknown;
};

function walk(node: unknown, visit: (n: Node) => void) {
  if (!node || typeof node !== 'object') return;
  if (Array.isArray(node)) return node.forEach((n) => walk(n, visit));
  const n = node as Node;
  if (typeof n.type === 'string') visit(n);
  for (const [key, value] of Object.entries(n)) {
    if (key !== 'parent' && key !== 'metadata') walk(value, visit);
  }
}

export function rawTextViolations(source: string): string[] {
  const ast = parse(source, { modern: true });
  const found: string[] = [];
  const attributeText = new Set<number>();
  walk(ast.fragment, (n) => {
    if (Array.isArray(n.value)) (n.value as Node[]).forEach((p) => attributeText.add(p.start));
  });
  walk(ast.fragment, (n) => {
    if (n.type === 'Text' && n.data && LETTER.test(n.data) && !attributeText.has(n.start))
      found.push(n.data.trim());
    if (n.type === 'Attribute' && n.name && TEXT_ATTRIBUTES.has(n.name) && Array.isArray(n.value)) {
      for (const part of n.value as Node[]) {
        if (part.type === 'Text' && part.data && LETTER.test(part.data))
          found.push(`${n.name}="${part.data}"`);
      }
    }
  });
  return found;
}

function leafKeys(tree: object, prefix = ''): string[] {
  return Object.entries(tree).flatMap(([k, v]) =>
    typeof v === 'string' || (v && typeof v === 'object' && 'other' in v)
      ? [`${prefix}${k}`]
      : leafKeys(v, `${prefix}${k}.`),
  );
}

describe('i18n lint', () => {
  it('detects raw text and text attributes', () => {
    expect(rawTextViolations('<p>Hello</p><b>{t("x")}</b><i title="Tip">·</i>')).toEqual([
      'Hello',
      'title="Tip"',
    ]);
    expect(rawTextViolations('<p>{t("x")} · 42</p><style>p{color:red}</style>')).toEqual([]);
  });

  it('components contain no hard-coded text', () => {
    const offenders = files(SRC, ['.svelte'])
      .map((f) => [relative(SRC, f), rawTextViolations(readFileSync(f, 'utf8'))] as const)
      .filter(([, v]) => v.length > 0);
    expect(offenders).toEqual([]);
  });

  it('every message key is used', () => {
    const sources = files(SRC, ['.svelte', '.ts'])
      .filter((f) => !f.includes('/i18n/') && !f.endsWith('.test.ts'))
      .map((f) => readFileSync(f, 'utf8'))
      .join('\n');
    const dynamic = [...sources.matchAll(/i18n-dynamic:\s*([\w.-]+)/g)].map((m) => m[1]!);
    const unused = leafKeys(en).filter(
      (key) =>
        !sources.includes(`'${key}'`) &&
        !sources.includes(`"${key}"`) &&
        !dynamic.some((p) => key.startsWith(p)),
    );
    expect(unused).toEqual([]);
  });
});
