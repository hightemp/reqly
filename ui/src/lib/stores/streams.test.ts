import { describe, expect, it, vi } from 'vitest';
import type { Transport } from '$lib/api/transport';
import { StreamStore } from './streams.svelte';

function fakeTransport(handlers: Record<string, (input: unknown) => unknown>) {
  const listeners: ((e: { topic: string; payload: unknown }) => void)[] = [];
  const transport = {
    kind: 'web',
    call: vi.fn(async (name: string, input: unknown) => handlers[name]?.(input)),
    subscribe: (l: (e: { topic: string; payload: unknown }) => void) => {
      listeners.push(l);
      return () => {};
    },
  } as unknown as Transport;
  const emit = (topic: string, payload: unknown) => listeners.forEach((l) => l({ topic, payload }));
  return { transport, emit };
}

const info = (status: string) => ({
  connectionId: 'c1',
  requestId: 'r1',
  kind: 'sse',
  status,
  headers: [],
  url: 'http://x',
  openedAt: 0,
});

const message = (seq: number) => ({
  seq,
  time: 0,
  direction: 'received',
  kind: 'event',
  data: `m${seq}`,
  size: 2,
});

describe('StreamStore', () => {
  it('keeps newer status events over the connect answer and merges the log', async () => {
    let emit: (topic: string, payload: unknown) => void = () => {};
    const fake = fakeTransport({
      'template.prompts': () => [],
      'stream.connect': () => {
        // The open event arrives before the answer.
        emit('stream.status', info('open'));
        emit('stream.message', { connectionId: 'c1', requestId: 'r1', message: message(2) });
        return info('connecting');
      },
      'stream.log': () => [message(1), message(2)],
    });
    emit = fake.emit;
    const store = new StreamStore(fake.transport, async () => {});
    const run = store.run('r1');
    await store.connect('r1');
    expect(run.info?.status).toBe('open');
    expect(run.messages.map((m) => m.seq)).toEqual([1, 2]);
    expect(run.open).toBe(true);

    fake.emit('stream.message', { connectionId: 'c1', requestId: 'r1', message: message(3) });
    fake.emit('stream.message', { connectionId: 'other', requestId: 'r1', message: message(9) });
    expect(run.messages.map((m) => m.seq)).toEqual([1, 2, 3]);
    fake.emit('stream.status', info('closed'));
    expect(run.open).toBe(false);
  });

  it('asks for prompts and cancels when dismissed', async () => {
    const fake = fakeTransport({
      'template.prompts': () => [{ key: 'k', label: 'K', defaultValue: '', masked: false }],
      'stream.connect': () => info('open'),
    });
    const store = new StreamStore(fake.transport, async () => {});
    store.askPrompts = async () => null;
    await store.connect('r1');
    expect(fake.transport.call).not.toHaveBeenCalledWith('stream.connect', expect.anything());
  });
});
