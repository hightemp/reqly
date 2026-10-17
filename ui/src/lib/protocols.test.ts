import { describe, expect, it } from 'vitest';
import { isSubscription, streamKind } from './protocols';

describe('protocols', () => {
  it('detects GraphQL subscriptions like the core', () => {
    expect(isSubscription('subscription { counter }')).toBe(true);
    expect(isSubscription('# query\nsubscription S { a }')).toBe(true);
    expect(isSubscription('query { a }')).toBe(false);
    expect(isSubscription('{ a }')).toBe(false);
    expect(isSubscription('query A { a } subscription B { b }', 'B')).toBe(true);
    expect(isSubscription('query A { a } subscription B { b }', 'A')).toBe(false);
  });

  it('knows which requests stream', () => {
    const http = { kind: 'request', eventStream: false, body: { kind: 'json' } } as never;
    expect(streamKind(http)).toBeNull();
    expect(
      streamKind({ kind: 'request', eventStream: true, body: { kind: 'none' } } as never),
    ).toBe('sse');
    expect(
      streamKind({
        kind: 'request',
        eventStream: false,
        body: { kind: 'graphql', graphql: { query: 'subscription { a }', operationName: '' } },
      } as never),
    ).toBe('graphqlSubscription');
    expect(streamKind({ kind: 'webSocket' } as never)).toBe('webSocket');
    expect(streamKind({ kind: 'grpc' } as never)).toBe('grpc');
    expect(streamKind({ kind: 'folder' } as never)).toBeNull();
  });
});
