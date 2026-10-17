// Field-name completion for JSON messages, from a sample message that has
// every field (gRPC method examples).

import type {
  CompletionContext,
  CompletionResult,
  CompletionSource,
} from '@codemirror/autocomplete';

type Json = null | boolean | number | string | Json[] | { [k: string]: Json };

/** Keys of objects in `sample`, by path (`a.b`, arrays transparent). */
export function keysByPath(sample: Json, path = '', out: Map<string, string[]> = new Map()) {
  if (Array.isArray(sample)) {
    for (const item of sample) keysByPath(item, path, out);
  } else if (sample && typeof sample === 'object') {
    out.set(path, Object.keys(sample));
    for (const [k, v] of Object.entries(sample)) keysByPath(v, path ? `${path}.${k}` : k, out);
  }
  return out;
}

/** Object path at `pos` in (possibly incomplete) JSON text: keys of enclosing objects. */
export function pathAt(text: string, pos: number): string {
  // Key of each open container; containers inside arrays have none.
  const stack: (string | null)[] = [];
  let lastKey: string | null = null;
  let i = 0;
  while (i < pos) {
    const c = text[i]!;
    if (c === '"') {
      let j = i + 1;
      let s = '';
      while (j < text.length && text[j] !== '"') {
        if (text[j] === '\\') j++;
        s += text[j] ?? '';
        j++;
      }
      if (j >= pos) break; // cursor inside this string
      // A key if followed by a colon.
      let k = j + 1;
      while (k < text.length && /\s/.test(text[k]!)) k++;
      if (text[k] === ':') lastKey = s;
      i = j + 1;
      continue;
    }
    if (c === '{' || c === '[') {
      stack.push(lastKey);
      lastKey = null;
    } else if (c === '}' || c === ']') {
      stack.pop();
    } else if (c === ',') {
      lastKey = null;
    }
    i++;
  }
  // The outermost `{` has no key.
  return stack
    .slice(1)
    .filter((k): k is string => k !== null)
    .join('.');
}

export function messageCompletions(sample: () => string | null): CompletionSource {
  return (ctx: CompletionContext): CompletionResult | null => {
    const word = ctx.matchBefore(/"?[\w]*/);
    if (!word || (word.from === word.to && !ctx.explicit)) return null;
    const text = sample();
    if (!text) return null;
    let parsed: Json;
    try {
      parsed = JSON.parse(text) as Json;
    } catch {
      return null;
    }
    const keys = keysByPath(parsed).get(pathAt(ctx.state.doc.toString(), word.from)) ?? [];
    if (!keys.length) return null;
    const quoted = word.text.startsWith('"');
    return {
      from: word.from,
      options: keys.map((k) => ({
        label: quoted ? `"${k}"` : k,
        apply: `"${k}": `,
        type: 'property',
      })),
      validFor: /^"?\w*$/,
    };
  };
}
