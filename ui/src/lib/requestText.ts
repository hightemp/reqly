// Recognizes requests in pasted text: cURL commands and HTTP request text.

const METHODS = 'GET|POST|PUT|PATCH|DELETE|HEAD|OPTIONS|TRACE|CONNECT|QUERY';
const REQUEST_LINE = new RegExp(`^(${METHODS})\\s+\\S+(\\s+HTTP/[\\d.]+)?\\s*$`);

/**
 * What a paste into the URL bar holds: a cURL command, HTTP request text
 * (a request line plus headers or a version, or a `###` block), or neither.
 */
export function pastedRequest(text: string): 'curl' | 'http' | null {
  if (/^\s*curl(\.exe)?\s/i.test(text)) return 'curl';
  const lines = text.trim().split(/\r?\n/);
  const first = lines[0]?.trim() ?? '';
  if (first.startsWith('###')) return 'http';
  const line = REQUEST_LINE.exec(first);
  if (line && (lines.length > 1 || line[2])) return 'http';
  return null;
}

/** Text worth importing as requests: a pasted request or a bare request line. */
export function isRequestText(text: string): boolean {
  return pastedRequest(text) !== null || REQUEST_LINE.test(text.trim().split(/\r?\n/)[0] ?? '');
}
