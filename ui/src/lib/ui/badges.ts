import type { NodeData } from '$lib/api/generated/NodeData';
import type { Protocol } from '$lib/api/generated/Protocol';

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

const PROTOCOL_BADGES: Partial<Record<Protocol, { label: string; color: string }>> = {
  graphql: { label: 'GQL', color: 'var(--c-protocol-graphql, #e535ab)' },
  sse: { label: 'SSE', color: 'var(--c-protocol-sse, #f59e0b)' },
  webSocket: { label: 'WS', color: 'var(--c-protocol-ws, #14b8a6)' },
  grpc: { label: 'gRPC', color: 'var(--c-protocol-grpc, #6366f1)' },
};

/** Badge for a request: the protocol for non-HTTP ones, else the method. */
export function requestBadge(
  method: string | null | undefined,
  protocol?: Protocol | null,
): { label: string; color: string; title: string } {
  const p = protocol ? PROTOCOL_BADGES[protocol] : undefined;
  if (p) return { ...p, title: p.label };
  const m = method || 'GET';
  return { label: methodLabel(m), color: methodColorVar(m), title: m.toUpperCase() };
}

/** Protocol of stored request data. */
export function protocolOf(data: NodeData | { kind: string }): Protocol | null {
  switch (data.kind) {
    case 'webSocket':
      return 'webSocket';
    case 'grpc':
      return 'grpc';
    case 'request': {
      const r = data as Extract<NodeData, { kind: 'request' }>;
      if (r.body?.kind === 'graphql') return 'graphql';
      return r.eventStream ? 'sse' : 'http';
    }
    default:
      return null;
  }
}

/** HTTP method of stored request data (`GET` for other protocols). */
export function methodOf(data: { kind: string; method?: unknown }): string {
  return typeof data.method === 'string' && data.method ? data.method : 'GET';
}
