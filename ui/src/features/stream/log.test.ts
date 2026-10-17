import { describe, expect, it } from 'vitest';
import type { StreamMessage } from '$lib/api/generated/StreamMessage';
import { eventTypes, filterMessages, prettyJson, preview } from './log';

const msg = (
  seq: number,
  direction: StreamMessage['direction'],
  data: string,
  event?: string,
): StreamMessage => ({
  seq,
  time: 0,
  direction,
  kind: event ? 'event' : 'text',
  data,
  event,
  size: data.length,
});

describe('stream log', () => {
  const log = [
    msg(1, 'info', 'connected'),
    msg(2, 'sent', 'hello'),
    msg(3, 'received', '{"a":1}', 'update'),
    msg(4, 'received', 'bye', 'done'),
  ];

  it('filters by direction, text and event type', () => {
    expect(filterMessages(log, { text: '', direction: 'all', event: '' })).toHaveLength(4);
    expect(
      filterMessages(log, { text: '', direction: 'received', event: '' }).map((m) => m.seq),
    ).toEqual([3, 4]);
    expect(
      filterMessages(log, { text: 'HEL', direction: 'all', event: '' }).map((m) => m.seq),
    ).toEqual([2]);
    expect(
      filterMessages(log, { text: '', direction: 'all', event: 'done' }).map((m) => m.seq),
    ).toEqual([4]);
    expect(eventTypes(log)).toEqual(['update', 'done']);
  });

  it('formats', () => {
    expect(prettyJson('{"a":1}')).toBe('{\n  "a": 1\n}');
    expect(prettyJson('nope')).toBeNull();
    expect(preview(msg(1, 'sent', 'a\n  b'))).toBe('a b');
  });
});
