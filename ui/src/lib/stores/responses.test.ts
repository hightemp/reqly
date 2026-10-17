import { afterEach, describe, expect, it, vi } from 'vitest';
import { FakeTransport } from '$lib/testing/fakeTransport';
import { toasts } from '$lib/ui/overlay/toasts.svelte';
import { ApiError } from '$lib/api/transport';
import { ResponseStore } from './responses.svelte';

const record = (id: string, status = 200) => ({
  id,
  requestId: 'r1',
  status,
  statusText: 'OK',
  createdAt: 1,
  timings: { totalMs: 5 },
});
const summary = (id: string) => ({
  id,
  requestId: 'r1',
  createdAt: 1,
  status: 200,
  statusText: 'OK',
  totalMs: 5,
  bodySize: 1,
  method: 'GET',
  url: 'u',
  errorCode: null,
});

function setup() {
  const t = new FakeTransport();
  let history = [summary('old')];
  t.handle('response.list', () => history as never);
  t.handle('response.get', ({ id }) => record(id) as never);
  const flush = vi.fn(async () => {});
  const store = new ResponseStore(t, flush);
  return { t, store, flush, setHistory: (h: ReturnType<typeof summary>[]) => (history = h) };
}

afterEach(() => vi.restoreAllMocks());

describe('ResponseStore', () => {
  it('loads history and the latest record', async () => {
    const { store } = setup();
    const run = await store.load('r1');
    expect(run.history.map((h) => h.id)).toEqual(['old']);
    expect(run.current?.id).toBe('old');
  });

  it('sends after flushing the draft and shows the new record', async () => {
    const { t, store, flush, setHistory } = setup();
    t.handle('request.send', async () => {
      setHistory([summary('new'), summary('old')]);
      return record('new') as never;
    });
    await store.load('r1');
    const p = store.send('r1');
    expect(store.run('r1').sending).toBe(true);
    await p;
    expect(flush).toHaveBeenCalledWith('r1');
    expect(store.run('r1').sending).toBe(false);
    expect(store.run('r1').current?.id).toBe('new');
    expect(store.run('r1').history[0]!.id).toBe('new');
  });

  it('tracks progress events', async () => {
    const { t, store } = setup();
    await store.load('r1');
    t.emit({
      topic: 'request.progress',
      payload: { requestId: 'r1', downloaded: 500, total: 1000 },
    });
    expect(store.run('r1').progress).toEqual({ downloaded: 500, total: 1000 });
  });

  it('cancel calls the backend and a cancelled send is silent', async () => {
    const { t, store } = setup();
    const cancel = vi.fn(() => ({}));
    t.handle('request.cancel', cancel);
    t.handle('request.send', async () => {
      throw new ApiError({ code: 'http.cancelled', message: '', params: {} });
    });
    const error = vi.spyOn(toasts, 'error');
    await store.load('r1');
    const p = store.send('r1');
    await store.cancel('r1');
    await p;
    expect(cancel).toHaveBeenCalledWith({ requestId: 'r1' });
    expect(error).not.toHaveBeenCalled();
    expect(store.run('r1').sending).toBe(false);
  });

  it('other failures are reported', async () => {
    const { t, store } = setup();
    t.handle('request.send', async () => {
      throw new ApiError({
        code: 'not_found',
        message: 'gone',
        params: { entity: 'node', id: 'r1' },
      });
    });
    const error = vi.spyOn(toasts, 'error').mockImplementation(() => 0);
    await store.load('r1');
    await store.send('r1');
    expect(error).toHaveBeenCalled();
  });

  it('selects a past response and clears history', async () => {
    const { t, store, setHistory } = setup();
    t.handle('history.clear', () => {
      setHistory([]);
      return {};
    });
    await store.load('r1');
    await store.select('r1', 'old');
    expect(store.run('r1').current?.id).toBe('old');
    await store.clear('r1');
    expect(store.run('r1').history).toEqual([]);
    expect(store.run('r1').current).toBeNull();
  });

  it('refreshes when another client finishes a request', async () => {
    const { t, store, setHistory } = setup();
    await store.load('r1');
    setHistory([summary('remote'), summary('old')]);
    t.emit({ topic: 'request.finished', payload: { requestId: 'r1', responseId: 'remote' } });
    await vi.waitFor(() => expect(store.run('r1').current?.id).toBe('remote'));
  });
});
