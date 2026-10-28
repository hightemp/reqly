import { describe, expect, it } from 'vitest';
import type { RunItem } from '$lib/api/generated/RunItem';
import type { RunReport } from '$lib/api/generated/RunReport';
import type { RunnerInput } from '$lib/api/generated/RunnerInput';
import { FakeTransport } from '$lib/testing/fakeTransport';
import { RunnerStore, failedRequestIds } from './runner.svelte';

const item = (name: string, passed = true, iteration = 0): RunItem => ({
  iteration,
  requestId: `id-${name}`,
  name,
  path: [],
  method: 'GET',
  url: `http://x/${name}`,
  status: 200,
  tests: passed ? [] : [{ name: 't', passed: false, skipped: false, error: 'no' }],
  assertions: [],
});

function report(runId: string, items: RunItem[]): RunReport {
  return {
    runId,
    name: 'C',
    startedAt: 0,
    finishedAt: 10,
    iterations: 1,
    items,
    cancelled: false,
    stopped: false,
  };
}

describe('RunnerStore', () => {
  it('collects progress for its run and keeps the report', async () => {
    const transport = new FakeTransport();
    let release!: () => void;
    transport.handle('runner.run', async (input: RunnerInput) => {
      transport.emit({
        topic: 'runner.progress',
        payload: { runId: 'other', total: 9, done: 1, item: item('x') },
      });
      transport.emit({
        topic: 'runner.progress',
        payload: { runId: input.runId, total: 2, done: 1, item: item('a') },
      });
      await new Promise<void>((r) => (release = r));
      return report(input.runId, [item('a'), item('b', false)]);
    });
    const store = new RunnerStore(transport);
    store.open({ collectionId: 'c1', folderId: null, name: 'C' });
    const done = store.start({ collectionId: 'c1', requestIds: ['id-a', 'id-b'] });
    await Promise.resolve();
    expect(store.status).toBe('running');
    expect(store.items.map((i) => i.name)).toEqual(['a']);
    expect(store.total).toBe(2);
    release();
    await done;
    expect(store.status).toBe('done');
    expect(store.report?.items).toHaveLength(2);
    expect(store.items.map((i) => i.name)).toEqual(['a', 'b']);
  });

  it('cancels its run and reports errors', async () => {
    const transport = new FakeTransport();
    transport.handle('runner.cancel', () => ({}));
    transport.handle('runner.run', () => Promise.reject(new Error('boom')));
    const store = new RunnerStore(transport);
    await store.start({ collectionId: 'c1' });
    expect(store.status).toBe('done');
    expect(store.error).toBeTruthy();

    transport.handle('runner.run', () => new Promise(() => {}));
    void store.start({ collectionId: 'c1' });
    await store.cancel();
    expect(transport.calls.at(-1)).toEqual({
      name: 'runner.cancel',
      input: { runId: store.runId },
    });
  });

  it('reruns the failed requests with the same options', async () => {
    const transport = new FakeTransport();
    const inputs: RunnerInput[] = [];
    transport.handle('runner.run', (input: RunnerInput) => {
      inputs.push(input);
      return report(input.runId, [
        item('a'),
        item('b', false),
        item('a', false, 1),
        item('b', false, 1),
      ]);
    });
    const store = new RunnerStore(transport);
    await store.start({ collectionId: 'c1', requestIds: ['id-a', 'id-b'], delayMs: 5 });
    expect(failedRequestIds(store.report!)).toEqual(['id-b', 'id-a']);
    await store.rerunFailed();
    expect(inputs[1]).toMatchObject({
      collectionId: 'c1',
      requestIds: ['id-b', 'id-a'],
      delayMs: 5,
    });
    expect(inputs[1]!.runId).not.toBe(inputs[0]!.runId);
  });
});
