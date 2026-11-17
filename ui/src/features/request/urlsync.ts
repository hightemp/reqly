// URL bar ↔ params table. The stored `url` has no query string; query
// parameters live in `params` (so disabled ones survive edits of the URL).

import type { KeyValue } from '$lib/api/generated/KeyValue';

const safeDecode = (s: string) => {
  try {
    return decodeURIComponent(s.replace(/\+/g, ' '));
  } catch {
    return s;
  }
};

/** What the URL bar shows: base URL plus enabled params (unencoded). */
export function displayUrl(url: string, params: KeyValue[]): string {
  const query = params
    .filter((p) => p.enabled && p.name)
    .map((p) => (p.value === '' ? p.name : `${p.name}=${p.value}`))
    .join('&');
  if (!query) return url;
  const [base, fragment] = splitFragment(url);
  return `${base}?${query}${fragment}`;
}

function splitFragment(url: string): [string, string] {
  const i = url.indexOf('#');
  return i < 0 ? [url, ''] : [url.slice(0, i), url.slice(i)];
}

/** Parses URL bar text into the stored url and params. */
export function applyUrlInput(
  input: string,
  current: KeyValue[],
  newId: () => string,
): { url: string; params: KeyValue[] } {
  const [withoutFragment, fragment] = splitFragment(input);
  const q = withoutFragment.indexOf('?');
  const base = q < 0 ? withoutFragment : withoutFragment.slice(0, q);
  const query = q < 0 ? '' : withoutFragment.slice(q + 1);

  const parsed = query
    .split('&')
    .filter((part) => part !== '')
    .map((part) => {
      const eq = part.indexOf('=');
      return eq < 0
        ? { name: safeDecode(part), value: '' }
        : { name: safeDecode(part.slice(0, eq)), value: safeDecode(part.slice(eq + 1)) };
    });

  // Walk the current rows: enabled ones take the parsed values in order (ids
  // stay stable while typing), disabled ones keep their place.
  const params: KeyValue[] = [];
  let next = 0;
  for (const row of current) {
    if (!row.enabled) params.push(row);
    else if (next < parsed.length) params.push({ ...row, ...parsed[next++]! });
  }
  for (const p of parsed.slice(next)) {
    params.push({
      id: newId(),
      enabled: true,
      name: p.name,
      value: p.value,
      description: '',
      kind: 'text',
    });
  }
  return { url: base + fragment, params };
}

/** `:name` path segments (port numbers and query values are ignored). */
export function pathParamNames(url: string): string[] {
  const path = url.split(/[?#]/)[0] ?? '';
  const afterScheme = path.replace(/^[a-z][a-z0-9+.-]*:\/\//i, '');
  const slash = afterScheme.indexOf('/');
  if (slash < 0) return [];
  return afterScheme
    .slice(slash + 1)
    .split('/')
    .filter((seg) => /^:[A-Za-z_][\w-]*$/.test(seg))
    .map((seg) => seg.slice(1));
}

/** Path param rows matching the URL; values of known names are kept. */
export function syncPathParams(url: string, current: KeyValue[], newId: () => string): KeyValue[] {
  return pathParamNames(url).map(
    (name) =>
      current.find((p) => p.name === name) ?? {
        id: newId(),
        enabled: true,
        name,
        value: '',
        description: '',
        kind: 'text',
      },
  );
}
