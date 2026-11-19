// In-app help: Markdown pages in content/<locale>/<topic>.md, bundled with
// the app. A page's first line is its `# Title`; `help:topic#anchor` links
// lead to other pages.

export const TOPIC_IDS = [
  'getting-started',
  'requests',
  'variables',
  'auth',
  'protocols',
  'import-export',
  'scripts',
  'script-api',
  'script-libraries',
  'testing',
  'collaboration',
  'sync',
  'mcp',
  'ai',
  'settings',
  'shortcuts',
] as const;

export type TopicId = (typeof TOPIC_IDS)[number];

export interface Topic {
  id: string;
  /** The language of the page (English when there is no translation). */
  locale: string;
  title: string;
  /** Markdown without the title. */
  body: string;
}

const files = import.meta.glob('./content/*/*.md', {
  query: '?raw',
  import: 'default',
  eager: true,
}) as Record<string, string>;

function parse(id: string, locale: string): Topic | undefined {
  const source = files[`./content/${locale}/${id}.md`];
  if (source === undefined) return undefined;
  const [first = '', ...rest] = source.replace(/\r\n/g, '\n').split('\n');
  return { id, locale, title: first.replace(/^#\s+/, '').trim(), body: rest.join('\n') };
}

export function topic(locale: string, id: string): Topic | undefined {
  return parse(id, locale) ?? parse(id, 'en');
}

export function topics(locale: string): Topic[] {
  return TOPIC_IDS.map((id) => topic(locale, id)).filter((t): t is Topic => Boolean(t));
}

export interface SearchHit {
  id: string;
  title: string;
  /** Text around the first match in the page (empty for a title match). */
  snippet: string;
}

/** Markdown reduced to plain text for searching and snippets. */
function plain(markdown: string): string {
  return markdown
    .replace(/```[\s\S]*?```/g, ' ')
    .replace(/\[([^\]]+)\]\([^)]*\)/g, '$1')
    .replace(/[#*`|>]/g, ' ')
    .replace(/\s+/g, ' ')
    .trim();
}

/** Pages whose title (first) or text contains `query`, ignoring case. */
export function searchTopics(list: Topic[], query: string): SearchHit[] {
  const q = query.trim().toLowerCase();
  if (!q) return [];
  const byTitle: SearchHit[] = [];
  const byText: SearchHit[] = [];
  for (const t of list) {
    if (t.title.toLowerCase().includes(q)) {
      byTitle.push({ id: t.id, title: t.title, snippet: '' });
      continue;
    }
    const text = plain(t.body);
    const at = text.toLowerCase().indexOf(q);
    if (at < 0) continue;
    const start = Math.max(0, at - 40);
    const end = Math.min(text.length, at + q.length + 60);
    const snippet = `${start > 0 ? '…' : ''}${text.slice(start, end)}${end < text.length ? '…' : ''}`;
    byText.push({ id: t.id, title: t.title, snippet });
  }
  return [...byTitle, ...byText];
}
