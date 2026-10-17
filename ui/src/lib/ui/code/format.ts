// Formatters that never change values: JSON is re-indented token by token,
// so big integers and exact number formatting survive (JSON.parse would not).

/** Pretty-prints JSON; returns null when the input is not valid JSON. */
export function formatJson(input: string, indent = 2): string | null {
  try {
    JSON.parse(input);
  } catch {
    return null;
  }
  const pad = (level: number) => ' '.repeat(level * indent);
  let out = '';
  let level = 0;
  let i = 0;
  while (i < input.length) {
    const c = input[i]!;
    if (c === '"') {
      let j = i + 1;
      while (j < input.length && input[j] !== '"') j += input[j] === '\\' ? 2 : 1;
      out += input.slice(i, j + 1);
      i = j + 1;
      continue;
    }
    if (/\s/.test(c)) {
      i++;
      continue;
    }
    if (c === '{' || c === '[') {
      // Keep empty containers compact.
      let j = i + 1;
      while (j < input.length && /\s/.test(input[j]!)) j++;
      const close = c === '{' ? '}' : ']';
      if (input[j] === close) {
        out += c + close;
        i = j + 1;
        continue;
      }
      level++;
      out += `${c}\n${pad(level)}`;
    } else if (c === '}' || c === ']') {
      level--;
      out += `\n${pad(level)}${c}`;
    } else if (c === ',') {
      out += `,\n${pad(level)}`;
    } else if (c === ':') {
      out += ': ';
    } else {
      out += c;
    }
    i++;
  }
  return out;
}

/** Simple XML pretty printer for display (tags on their own lines, text inline). */
export function formatXml(input: string, indent = 2): string {
  const tokens = input.replace(/>\s+</g, '><').match(/<[^>]+>|[^<]+/g) ?? [];
  const lines: string[] = [];
  let level = 0;
  for (let i = 0; i < tokens.length; i++) {
    const t = tokens[i]!.trim();
    if (!t) continue;
    const isClose = t.startsWith('</');
    const isSelf = t.endsWith('/>') || t.startsWith('<?') || t.startsWith('<!');
    const isOpen = t.startsWith('<') && !isClose && !isSelf;
    if (isClose) level = Math.max(0, level - 1);
    // <tag>text</tag> stays on one line.
    const next = tokens[i + 1];
    const after = tokens[i + 2];
    if (isOpen && next && !next.startsWith('<') && after?.startsWith('</')) {
      lines.push(' '.repeat(level * indent) + t + next.trim() + after.trim());
      i += 2;
      continue;
    }
    lines.push(' '.repeat(level * indent) + t);
    if (isOpen) level++;
  }
  return lines.join('\n');
}
