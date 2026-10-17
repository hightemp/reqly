// Sidebar search: free words match name or URL; `method:` and `url:` narrow down.

import type { RequestTreeItem } from '$lib/stores/tree.svelte';

export interface ParsedQuery {
  method: string | null;
  url: string[];
  words: string[];
}

export function parseQuery(query: string): ParsedQuery {
  const parsed: ParsedQuery = { method: null, url: [], words: [] };
  for (const token of query.trim().split(/\s+/).filter(Boolean)) {
    const lower = token.toLowerCase();
    if (lower.startsWith('method:') && token.length > 7)
      parsed.method = token.slice(7).toUpperCase();
    else if (lower.startsWith('url:') && token.length > 4)
      parsed.url.push(token.slice(4).toLowerCase());
    else parsed.words.push(lower);
  }
  return parsed;
}

export function makeFilter(query: string): ((item: RequestTreeItem) => boolean) | undefined {
  const q = parseQuery(query);
  if (!q.method && q.url.length === 0 && q.words.length === 0) return undefined;
  return (item) => {
    if (item.isFolder) {
      return (
        !q.method &&
        q.url.length === 0 &&
        q.words.every((w) => item.label.toLowerCase().includes(w))
      );
    }
    if (q.method && item.method?.toUpperCase() !== q.method) return false;
    const url = (item.url ?? '').toLowerCase();
    if (!q.url.every((u) => url.includes(u))) return false;
    const name = item.label.toLowerCase();
    return q.words.every((w) => name.includes(w) || url.includes(w));
  };
}

export interface Segment {
  text: string;
  match: boolean;
}

export function highlight(text: string, words: string[]): Segment[] {
  const needles = words.filter(Boolean).map((w) => w.toLowerCase());
  if (needles.length === 0) return [{ text, match: false }];
  const lower = text.toLowerCase();
  const marks = new Array<boolean>(text.length).fill(false);
  for (const n of needles) {
    let at = lower.indexOf(n);
    while (at >= 0) {
      for (let i = at; i < at + n.length; i++) marks[i] = true;
      at = lower.indexOf(n, at + n.length);
    }
  }
  const segments: Segment[] = [];
  for (let i = 0; i < text.length; i++) {
    const last = segments.at(-1);
    if (last && last.match === marks[i]) last.text += text[i];
    else segments.push({ text: text[i]!, match: marks[i]! });
  }
  return segments.length ? segments : [{ text, match: false }];
}
