// Pure logic of the tag editor: opening a tag, which arguments to show and
// what to write back.

import type { ArgInfo } from '$lib/api/generated/ArgInfo';
import type { FunctionInfo } from '$lib/api/generated/FunctionInfo';
import { parseTag, type ParsedTag, type TagArg } from '$lib/template/scan';

/** Insomnia keeps response filters as `b64::<base64>::46b`; the core reads both. */
export function decodeFilter(value: string): string {
  const m = /^b64::(.*)::46b$/.exec(value);
  if (!m) return value;
  try {
    const bytes = Uint8Array.from(atob(m[1]!), (c) => c.charCodeAt(0));
    return new TextDecoder('utf-8', { fatal: true }).decode(bytes);
  } catch {
    return value;
  }
}

/** The tag to edit: `text`, or a new `response` tag (the usual reason to open the editor). */
export function openTag(text: string, functions: FunctionInfo[]): ParsedTag {
  const parsed = parseTag(text);
  if (!parsed) {
    const first = functions.find((f) => f.name === 'response') ?? functions[0];
    return first ? startTag(first) : { name: 'uuid', args: [] };
  }
  if (parsed.name === 'response' && parsed.args[2]) {
    const args = [...parsed.args];
    args[2] = { ...args[2]!, value: decodeFilter(args[2]!.value) };
    return { ...parsed, args };
  }
  return parsed;
}

/** A new tag of `info`: required choices start at their first value. */
export function startTag(info: FunctionInfo): ParsedTag {
  const args: TagArg[] = [];
  for (const p of info.params) {
    if (p.optional) break;
    args.push({ kind: 'string', value: p.kind === 'choice' ? (p.choices[0] ?? '') : '' });
  }
  return { name: info.name, args };
}

const value = (tag: ParsedTag, i: number) => tag.args[i]?.value ?? '';

/** The arguments worth showing for the current values. */
export function visibleParams(info: FunctionInfo, tag: ParsedTag): ArgInfo[] {
  if (info.name === 'response') {
    const field = value(tag, 0);
    return info.params.filter((p) => {
      if (p.name === 'path') return field === 'body' || field === 'header';
      if (p.name === 'maxAgeSeconds') return value(tag, 3) === 'when-expired';
      return true;
    });
  }
  if (info.name === 'now') {
    return info.params.filter((p) => p.name !== 'pattern' || value(tag, 0) === 'custom');
  }
  if (info.name === 'os') {
    return info.params.filter((p) => p.name !== 'name' || value(tag, 0) === 'env');
  }
  return info.params;
}

/** The tag to write: numbers as numbers, no empty optional arguments at the end. */
export function finishTag(tag: ParsedTag, info: FunctionInfo | undefined): ParsedTag {
  const args = tag.args.map((a, i) => {
    const p = info?.params[i];
    if (p?.kind === 'number' && a.kind === 'string' && /^-?\d+(\.\d+)?$/.test(a.value.trim())) {
      return { kind: 'number' as const, value: a.value.trim() };
    }
    return a;
  });
  while (args.length) {
    const last = args.at(-1)!;
    const optional = info ? (info.params[args.length - 1]?.optional ?? true) : true;
    if (!optional || last.kind !== 'string' || last.value !== '') break;
    args.pop();
  }
  return { ...tag, args };
}

export interface RequestOption {
  value: string;
  label: string;
  method?: string | null;
}

interface Item {
  id: string;
  parentId: string | null;
  isFolder: boolean;
  label: string;
  method?: string | null;
}

/** Requests of the tree with their folder path, for the request picker. */
export function requestOptions(items: Item[]): RequestOption[] {
  const byId = new Map(items.map((i) => [i.id, i]));
  const path = (item: Item) => {
    const parts = [item.label];
    let parent = item.parentId ? byId.get(item.parentId) : undefined;
    for (let n = 0; parent && n < 64; n++) {
      parts.unshift(parent.label);
      parent = parent.parentId ? byId.get(parent.parentId) : undefined;
    }
    return parts.join(' / ');
  };
  return items
    .filter((i) => !i.isFolder)
    .map((i) => ({ value: i.id, label: path(i), method: i.method }));
}
