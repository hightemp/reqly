const KNOWN_METHODS = ['get', 'post', 'put', 'patch', 'delete', 'head', 'options'] as const;

/** CSS color for an HTTP method badge (theme token). */
export function methodColorVar(method: string): string {
  const m = method.toLowerCase();
  return `var(--c-method-${(KNOWN_METHODS as readonly string[]).includes(m) ? m : 'other'})`;
}

/** Short label so badges keep a fixed width, like Insomnia. */
export function methodLabel(method: string): string {
  const m = method.toUpperCase();
  return ({ DELETE: 'DEL', OPTIONS: 'OPT', PATCH: 'PTCH' } as Record<string, string>)[m] ?? m;
}

export type StatusClass = '1xx' | '2xx' | '3xx' | '4xx' | '5xx' | 'error';

export function statusClass(status: number | null | undefined): StatusClass {
  if (!status || status < 100 || status > 599) return 'error';
  return `${Math.floor(status / 100)}xx` as StatusClass;
}
