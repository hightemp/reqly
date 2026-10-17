import { afterEach, describe, expect, it, vi } from 'vitest';
import { FakeTransport } from '$lib/testing/fakeTransport';
import { EditorStore } from './editor.svelte';

function node(id: string, version = 1, url = 'http://a') {
  return {
    id,
    version,
    collectionId: 'c1',
    parentId: null,
    name: 'Req',
    sortKey: 'V',
    createdAt: 0,
    updatedAt: 0,
    deletedAt: null,
    data: {
      kind: 'request',
      method: 'GET',
      url,
      params: [],
      pathParams: [],
      headers: [],
      body: { kind: 'none', text: '', fields: [], file: null },
      auth: { type: 'inherit' },
      settings: {},
      description: '',
    },
  };
}

function setup(autosave = true) {
  const t = new FakeTransport();
  let stored = node('r1');
  const update = vi.fn(async (input: { id: string; name?: string; data?: unknown }) => {
    stored = {
      ...stored,
      version: stored.version + 1,
      name: input.name ?? stored.name,
      data: (input.data as never) ?? stored.data,
    };
    return stored as never;
  });
  t.handle('node.get', () => stored as never);
  t.handle('node.update', update as never);
  const store = new EditorStore(t, () => autosave);
  return { t, store, update, setStored: (n: ReturnType<typeof node>) => (stored = n) };
}

describe('EditorStore', () => {
  afterEach(() => vi.useRealTimers());

  it('loads a draft once', async () => {
    const { store, t } = setup();
    const a = await store.open('r1');
    const b = await store.open('r1');
    expect(a).toBe(b);
    expect(t.calls.filter((c) => c.name === 'node.get')).toHaveLength(1);
    expect(a.request?.url).toBe('http://a');
  });

  it('debounces autosave and sends the latest data', async () => {
    vi.useFakeTimers();
    const { store, update } = setup();
    const d = await store.open('r1');
    store.editRequest('r1', (r) => ({ ...r, url: 'http://b' }));
    store.editRequest('r1', (r) => ({ ...r, url: 'http://c' }));
    expect(d.dirty).toBe(true);
    await vi.advanceTimersByTimeAsync(600);
    expect(update).toHaveBeenCalledOnce();
    expect(update.mock.calls[0]![0]).toMatchObject({ id: 'r1', data: { url: 'http://c' } });
    expect(d.dirty).toBe(false);
    expect(d.node?.version).toBe(2);
    vi.useRealTimers();
  });

  it('without autosave only saves on demand', async () => {
    vi.useFakeTimers();
    const { store, update } = setup(false);
    const d = await store.open('r1');
    store.rename('r1', 'Renamed');
    await vi.advanceTimersByTimeAsync(1000);
    expect(update).not.toHaveBeenCalled();
    expect(d.dirty).toBe(true);
    await store.save('r1');
    expect(update).toHaveBeenCalledWith(expect.objectContaining({ name: 'Renamed' }));
    expect(d.dirty).toBe(false);
    vi.useRealTimers();
  });

  it('flush saves pending changes immediately', async () => {
    const { store, update } = setup();
    await store.open('r1');
    store.editRequest('r1', (r) => ({ ...r, method: 'POST' }));
    await store.flush('r1');
    expect(update).toHaveBeenCalledOnce();
  });

  it('edits made while saving are saved afterwards', async () => {
    vi.useFakeTimers();
    const { store, update, t } = setup();
    // Hold the first save in flight until released.
    let release!: () => void;
    const gate = new Promise<void>((r) => (release = r));
    const original = update.getMockImplementation()!;
    update.mockImplementationOnce(async (input) => {
      await gate;
      return original(input);
    });
    t.handle('node.update', update as never);
    await store.open('r1');
    store.editRequest('r1', (r) => ({ ...r, url: 'http://1' }));
    const first = store.flush('r1');
    await vi.waitFor(() => expect(update).toHaveBeenCalledOnce());
    store.editRequest('r1', (r) => ({ ...r, url: 'http://2' }));
    release();
    await first;
    await vi.advanceTimersByTimeAsync(600);
    expect(update).toHaveBeenCalledTimes(2);
    expect(update.mock.calls[1]![0]).toMatchObject({ data: { url: 'http://2' } });
    vi.useRealTimers();
  });

  it('reloads clean drafts when the node changes elsewhere', async () => {
    const { store, t, setStored } = setup();
    const d = await store.open('r1');
    setStored(node('r1', 5, 'http://remote'));
    t.emit({ topic: 'tree.changed', payload: { collectionIds: ['c1'], nodeIds: ['r1'] } });
    await vi.waitFor(() => expect(d.request?.url).toBe('http://remote'));
  });

  it('keeps local edits of dirty drafts on external change', async () => {
    const { store, t, setStored } = setup(false);
    const d = await store.open('r1');
    store.editRequest('r1', (r) => ({ ...r, url: 'http://mine' }));
    setStored(node('r1', 5, 'http://remote'));
    t.emit({ topic: 'tree.changed', payload: { collectionIds: ['c1'], nodeIds: ['r1'] } });
    await new Promise((r) => setTimeout(r, 0));
    expect(d.request?.url).toBe('http://mine');
  });
});
