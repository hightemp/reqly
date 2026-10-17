// Which requests open a connection instead of sending once.

import type { NodeData } from '$lib/api/generated/NodeData';

export type StreamKindName = 'webSocket' | 'sse' | 'graphqlSubscription' | 'grpc';

/** Whether the first (or named) operation of a GraphQL document is a subscription. */
export function isSubscription(query: string, operationName = ''): boolean {
  const clean = query
    .split('\n')
    .map((l) => l.split('#')[0])
    .join('\n');
  const tokens = clean.split(/[\s{(]+/).filter(Boolean);
  for (let i = 0; i < tokens.length; i++) {
    const t = tokens[i]!;
    if (t === 'query' || t === 'mutation' || t === 'subscription') {
      const name = tokens[i + 1] ?? '';
      if (!operationName.trim() || name === operationName.trim()) return t === 'subscription';
    }
  }
  return false;
}

/** The connection a request opens, or `null` for one-off HTTP requests. */
export function streamKind(data: NodeData | null | undefined): StreamKindName | null {
  if (!data) return null;
  switch (data.kind) {
    case 'webSocket':
      return 'webSocket';
    case 'grpc':
      return 'grpc';
    case 'request':
      if (data.body.kind === 'graphql')
        return isSubscription(data.body.graphql.query, data.body.graphql.operationName)
          ? 'graphqlSubscription'
          : null;
      return data.eventStream ? 'sse' : null;
    default:
      return null;
  }
}
