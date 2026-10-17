// Filtering and display helpers for connection logs.

import type { StreamMessage } from '$lib/api/generated/StreamMessage';

export type DirectionFilter = 'all' | 'sent' | 'received';

export interface LogFilter {
  text: string;
  direction: DirectionFilter;
  /** SSE event type; empty = all. */
  event: string;
}

export function filterMessages(messages: StreamMessage[], f: LogFilter): StreamMessage[] {
  const text = f.text.trim().toLowerCase();
  return messages.filter((m) => {
    if (f.direction !== 'all' && m.direction !== f.direction && m.direction !== 'info')
      return false;
    if (f.direction !== 'all' && m.direction === 'info') return false;
    if (f.event && (m.event ?? '') !== f.event) return false;
    if (
      text &&
      !m.data.toLowerCase().includes(text) &&
      !(m.event ?? '').toLowerCase().includes(text)
    )
      return false;
    return true;
  });
}

/** SSE event types seen in a log, in first-seen order. */
export function eventTypes(messages: StreamMessage[]): string[] {
  const seen = new Set<string>();
  for (const m of messages) if (m.event) seen.add(m.event);
  return [...seen];
}

/** One-line preview. */
export function preview(m: StreamMessage, max = 200): string {
  const line = m.data.replace(/\s+/g, ' ').trim();
  return line.length > max ? `${line.slice(0, max)}…` : line;
}

/** Pretty JSON when the data is JSON, else `null`. */
export function prettyJson(data: string): string | null {
  const t = data.trim();
  if (!t.startsWith('{') && !t.startsWith('[')) return null;
  try {
    return JSON.stringify(JSON.parse(t), null, 2);
  } catch {
    return null;
  }
}

/** `HH:MM:SS.mmm` in local time. */
export function clockTime(ms: number): string {
  const d = new Date(ms);
  const p = (n: number, w = 2) => String(n).padStart(w, '0');
  return `${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}.${p(d.getMilliseconds(), 3)}`;
}
