import { afterEach, describe, expect, it, vi } from 'vitest';
import { FakeTransport } from '$lib/testing/fakeTransport';
import { EditorStore, changes, rebase } from './editor.svelte';

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

function setup(autosave = true, readonly = false) {
  const t = new FakeTransport();
  let stored = node('r1');
  const update = vi.fn(
    async (input: { id: string; name?: string; fields?: Record<string, unknown> }) => {
      stored = {
        ...stored,
        version: stored.version + 1,
        name: input.name ?? stored.name,
        data: { ...stored.data, ...input.fields } as never,
      };
      return stored as never;
    },
  );
  t.handle('node.get', () => stored as never);
  t.handle('node.update', update as never);
  const store = new EditorStore(
    t,
    () => autosave,
    () => readonly,
  );
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
    expect(update.mock.calls[0]![0]).toEqual({
      id: 'r1',
      baseVersion: 1,
      fields: { url: 'http://c' },
    });
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

  it('never saves for viewers', async () => {
    vi.useFakeTimers();
    const { store, update } = setup(true, true);
    const d = await store.open('r1');
    store.editRequest('r1', (r) => ({ ...r, url: 'http://b' }));
    await vi.advanceTimersByTimeAsync(1000);
    await store.flush('r1');
    expect(update).not.toHaveBeenCalled();
    expect(d.request?.url).toBe('http://b');
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
    expect(update.mock.calls[1]![0]).toMatchObject({ baseVersion: 2, fields: { url: 'http://2' } });
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

  it("merges others' changes to other fields into a dirty draft", async () => {
    const { store, t, setStored, update } = setup(false);
    const d = await store.open('r1');
    store.editRequest('r1', (r) => ({ ...r, url: 'http://mine' }));
    const remote = node('r1', 5);
    setStored({ ...remote, data: { ...remote.data, method: 'POST' } });
    t.emit({ topic: 'tree.changed', payload: { collectionIds: ['c1'], nodeIds: ['r1'] } });
    await vi.waitFor(() => expect(d.request?.method).toBe('POST'));
    expect(d.request?.url).toBe('http://mine');
    await store.save('r1');
    // Only the local change is sent, based on the merged version.
    expect(update).toHaveBeenCalledWith({
      id: 'r1',
      baseVersion: 5,
      fields: { url: 'http://mine' },
    });
    expect(d.request?.method).toBe('POST');
  });

  it('shows who overwrote a change', async () => {
    const { t } = setup();
    const { toasts } = await import('$lib/ui/overlay/toasts.svelte');
    const show = vi.spyOn(toasts, 'show');
    t.emit({
      topic: 'node.overwritten',
      payload: { nodeId: 'r1', name: 'Req', fields: ['url'], by: 'Bob' },
    });
    expect(show).toHaveBeenCalledWith(expect.objectContaining({ kind: 'info' }));
    expect(show.mock.calls[0]![0].message).toContain('Bob');
  });
});

describe('changes and rebase', () => {
  it('leave fields edited together to the server', () => {
    const base = node('r1', 1);
    const body = { ...base.data.body, text: 'typed' };
    const local = { ...base, data: { ...base.data, body } };
    expect(changes(base as never, local as never, ['body.text']).fields).toEqual({});
    const kind = { ...local, data: { ...local.data, body: { ...body, kind: 'json' } } };
    expect(changes(base as never, kind as never, ['body.text']).fields).toEqual({
      body: { ...base.data.body, kind: 'json' },
    });
    const fresh = node('r1', 4, 'http://new');
    const merged = rebase(local as never, base as never, fresh as never, [
      'body.text',
    ]) as never as typeof local;
    expect(merged.data.url).toBe('http://new');
    expect(merged.data.body.text).toBe('typed');
  });

  it('diffs top-level fields and replays them on a fresh node', () => {
    const base = node('r1', 1, 'http://a');
    const local = { ...base, name: 'Mine', data: { ...base.data, url: 'http://b' } };
    expect(changes(base as never, local as never)).toEqual({
      fields: { url: 'http://b' },
      name: 'Mine',
    });
    const fresh = { ...node('r1', 3, 'http://a'), data: { ...base.data, method: 'PUT' } };
    const merged = rebase(local as never, base as never, fresh as never) as never as typeof local;
    expect(merged.version).toBe(3);
    expect(merged.name).toBe('Mine');
    expect(merged.data).toMatchObject({ url: 'http://b', method: 'PUT' });
  });
});
