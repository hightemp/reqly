// Minimal, safe Markdown → HTML for request/folder descriptions and the
// in-app help. Input is escaped first; only a small subset of syntax is
// recognised.

export interface MarkdownOptions {
  /** Help pages: `help:topic#anchor` links and ids on headings. */
  help?: boolean;
  /**
   * Highlights a fenced code block: safe HTML, or `null` to show it as
   * plain (escaped) text.
   */
  highlight?: (code: string, lang: string) => string | null;
  /** Adds a copy button (`.md-copy`, without a handler) to code blocks. */
  copyLabel?: string;
}

const escape = (s: string) =>
  s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');

/** A heading's anchor: lowercase words joined by dashes (any script). */
export function slug(text: string): string {
  return text
    .toLowerCase()
    .replace(/[`*]/g, '')
    .replace(/[^\p{L}\p{N}]+/gu, '-')
    .replace(/^-|-$/g, '');
}

function inline(text: string, options: MarkdownOptions = {}): string {
  const codes: string[] = [];
  let out = escape(text).replace(/`([^`]+)`/g, (_, c: string) => {
    codes.push(`<code>${c}</code>`);
    return `\uE000${codes.length - 1}\uE000`;
  });
  out = out
    .replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>')
    .replace(/(^|[^*])\*([^*]+)\*/g, '$1<em>$2</em>')
    .replace(/(^|[^\p{L}\p{N}_])_([^_]+)_(?![\p{L}\p{N}_])/gu, '$1<em>$2</em>')
    .replace(/\[([^\]]+)\]\(((?:[^()\s]|\([^()\s]*\))+)\)/g, (_, label: string, href: string) => {
      const raw = href.replace(/&amp;/g, '&');
      const help = /^help:([\w-]+(?:#[\p{L}\p{N}-]+)?)$/u.exec(raw);
      if (help) {
        return options.help
          ? `<a href="#${escape(help[1]!)}" data-help="${escape(help[1]!)}">${label}</a>`
          : label;
      }
      return /^(https?:|mailto:)/i.test(raw)
        ? `<a href="${escape(raw)}" target="_blank" rel="noopener noreferrer">${label}</a>`
        : label;
    });
  return out.replace(/\uE000(\d+)\uE000/g, (_, i: string) => codes[Number(i)]!);
}

/** Cells of a table row: `| a | b |`; `\\|` is a literal pipe. */
function cells(line: string): string[] {
  return line
    .trim()
    .replace(/^\|/, '')
    .replace(/(?<!\\)\|$/, '')
    .split(/(?<!\\)\|/)
    .map((c) => c.trim().replace(/\\\|/g, '|'));
}

const TABLE_RULE = /^\s*\|?\s*:?-+:?\s*(\|\s*:?-+:?\s*)*\|?\s*$/;

export function renderMarkdown(source: string, options: MarkdownOptions = {}): string {
  const inline_ = (text: string) => inline(text, options);
  const lines = source.replace(/\r\n/g, '\n').split('\n');
  const blocks: string[] = [];
  let para: string[] = [];
  let list: { ordered: boolean; items: string[] } | null = null;

  const flushPara = () => {
    if (para.length) blocks.push(`<p>${inline_(para.join(' '))}</p>`);
    para = [];
  };
  const flushList = () => {
    if (list) {
      const tag = list.ordered ? 'ol' : 'ul';
      blocks.push(`<${tag}>${list.items.map((i) => `<li>${inline_(i)}</li>`).join('')}</${tag}>`);
    }
    list = null;
  };

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i]!;
    if (line.startsWith('```')) {
      flushPara();
      flushList();
      const lang = line.slice(3).trim().split(/\s+/)[0]!.toLowerCase();
      const code: string[] = [];
      for (i++; i < lines.length && !lines[i]!.startsWith('```'); i++) code.push(lines[i]!);
      const text = code.join('\n');
      const body = (lang ? options.highlight?.(text, lang) : null) ?? escape(text);
      const attr = lang ? ` data-lang="${escape(lang)}"` : '';
      const pre = `<pre><code${attr}>${body}</code></pre>`;
      if (options.copyLabel) {
        const label = escape(options.copyLabel);
        blocks.push(
          `<div class="md-code"><button type="button" class="md-copy" aria-label="${label}" title="${label}"></button>${pre}</div>`,
        );
      } else {
        blocks.push(pre);
      }
      continue;
    }
    const rule = lines[i + 1] ?? '';
    if (line.includes('|') && rule.includes('|') && TABLE_RULE.test(rule)) {
      flushPara();
      flushList();
      const head = cells(line);
      const rows: string[][] = [];
      for (i += 2; i < lines.length && lines[i]!.includes('|') && lines[i]!.trim(); i++) {
        rows.push(cells(lines[i]!));
      }
      i--;
      const tr = (row: string[], tag: string) =>
        `<tr>${row.map((c) => `<${tag}>${inline_(c)}</${tag}>`).join('')}</tr>`;
      blocks.push(
        `<table><thead>${tr(head, 'th')}</thead><tbody>${rows.map((r) => tr(r, 'td')).join('')}</tbody></table>`,
      );
      continue;
    }
    const heading = /^(#{1,6})\s+(.*)$/.exec(line);
    const bullet = /^\s*[-*]\s+(.*)$/.exec(line);
    const numbered = /^\s*\d+\.\s+(.*)$/.exec(line);
    if (heading) {
      flushPara();
      flushList();
      const level = heading[1]!.length;
      const id = options.help ? ` id="${escape(slug(heading[2]!))}"` : '';
      blocks.push(`<h${level}${id}>${inline_(heading[2]!)}</h${level}>`);
    } else if (list && !bullet && !numbered && /^\s+\S/.test(line)) {
      // An indented line continues the last list item.
      list.items[list.items.length - 1] += ` ${line.trim()}`;
    } else if (bullet || numbered) {
      flushPara();
      const ordered = Boolean(numbered);
      if (list && list.ordered !== ordered) flushList();
      list ??= { ordered, items: [] };
      list.items.push((bullet ?? numbered)![1]!);
    } else if (line.trim() === '') {
      flushPara();
      flushList();
    } else {
      flushList();
      para.push(line.trim());
    }
  }
  flushPara();
  flushList();
  return blocks.join('\n');
}
