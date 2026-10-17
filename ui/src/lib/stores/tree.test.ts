import { describe, expect, it, vi } from 'vitest';
import { dialogs } from '$lib/ui/overlay/dialogs.svelte';
import { toasts } from '$lib/ui/overlay/toasts.svelte';
import { FakeTransport } from '$lib/testing/fakeTransport';
import { TreeStore } from './tree.svelte';

type N = {
  id: string;
  parentId: string | null;
  kind: 'folder' | 'request';
  name: string;
  method: string | null;
  sortKey: string;
  createdAt: number;
  updatedAt: number;
};
const n = (
  id: string,
  parentId: string | null,
  kind: N['kind'],
  name: string,
  sortKey: string,
  method: string | null = null,
): N => ({
  id,
  parentId,
  kind,
  name,
  method,
  sortKey,
  createdAt: Number(sortKey.charCodeAt(0)),
  updatedAt: 0,
});

function backend(nodes: N[]) {
  const t = new FakeTransport();
  const settings = t.withSettings();
  const summaries = () =>
    nodes.map((x) => ({
      ...x,
      collectionId: 'c1',
      url: x.kind === 'request' ? 'http://x' : null,
      version: 1,
    }));
  t.handle('tree.list', () => summaries() as never);
  const spy = {
    create: vi.fn((input: unknown) => ({ id: 'new', ...(input as object) }) as never),
    update: vi.fn(() => ({}) as never),
    move: vi.fn(() => [] as never),
    copy: vi.fn(() => [] as never),
    duplicate: vi.fn(() => [] as never),
    delete: vi.fn(() => ({}) as never),
    restore: vi.fn(() => ({}) as never),
  };
  t.handle('node.create', spy.create);
  t.handle('node.update', spy.update);
  t.handle('tree.move', spy.move);
  t.handle('tree.copy', spy.copy);
  t.handle('tree.duplicate', spy.duplicate);
  t.handle('tree.delete', spy.delete);
  t.handle('tree.restore', spy.restore);
  t.handle('request.toCurl', ({ requestId }) => ({ command: `curl ${requestId}` }));
  return { t, spy, settings };
}

const sample = () => [
  n('api', null, 'folder', 'api', 'a'),
  n('b', 'api', 'request', 'beta', 'b', 'POST'),
  n('a', 'api', 'request', 'alpha', 'c', 'GET'),
  n('root-req', null, 'request', 'health', 'd', 'GET'),
];

async function setup(nodes = sample()) {
  const b = backend(nodes);
  const store = new TreeStore(b.t);
  await store.load('c1');
  return { ...b, store };
}

describe('TreeStore', () => {
  it('loads items keeping sort-key order within each parent', async () => {
    const { store } = await setup();
    const childrenOf = (p: string | null) =>
      store.items.filter((i) => i.parentId === p).map((i) => i.id);
    expect(childrenOf(null)).toEqual(['api', 'root-req']);
    expect(childrenOf('api')).toEqual(['b', 'a']);
    expect(store.byId.get('b')).toMatchObject({
      isFolder: false,
      label: 'beta',
      method: 'POST',
      parentId: 'api',
    });
  });

  it('applies per-folder sort modes', async () => {
    const { store } = await setup();
    await store.setSortMode('api', 'name');
    expect(store.items.filter((i) => i.parentId === 'api').map((i) => i.label)).toEqual([
      'alpha',
      'beta',
    ]);
    await store.setSortMode('api', 'method');
    expect(store.items.filter((i) => i.parentId === 'api').map((i) => i.method)).toEqual([
      'GET',
      'POST',
    ]);
  });

  it('persists expanded folders per collection', async () => {
    const { store, settings, t } = await setup();
    store.setExpanded(['api']);
    await store.flushUiState();
    expect(settings.get('ui.tree.c1')).toMatchObject({ expanded: ['api'] });
    const again = new TreeStore(t);
    await again.load('c1');
    expect(again.expanded).toEqual(['api']);
  });

  it('maps drop targets to parent and placement', async () => {
    const { store, spy } = await setup();
    await store.move(['a'], { id: 'api', position: 'inside' });
    expect(spy.move).toHaveBeenLastCalledWith({
      ids: ['a'],
      collectionId: 'c1',
      parentId: 'api',
      placement: { type: 'end' },
    });
    await store.move(['a'], { id: 'b', position: 'before' });
    expect(spy.move).toHaveBeenLastCalledWith({
      ids: ['a'],
      collectionId: 'c1',
      parentId: 'api',
      placement: { type: 'before', id: 'b' },
    });
    await store.move(['b'], { id: 'root-req', position: 'after' });
    expect(spy.move).toHaveBeenLastCalledWith({
      ids: ['b'],
      collectionId: 'c1',
      parentId: null,
      placement: { type: 'after', id: 'root-req' },
    });
    await store.move(['b'], { id: null, position: 'inside' });
    expect(spy.move).toHaveBeenLastCalledWith({
      ids: ['b'],
      collectionId: 'c1',
      parentId: null,
      placement: { type: 'end' },
    });
  });

  it('switches to manual sort when reordering inside a sorted folder', async () => {
    const { store } = await setup();
    await store.setSortMode('api', 'name');
    await store.move(['a'], { id: 'b', position: 'after' });
    expect(store.sortModeOf('api')).toBe('manual');
  });

  it('creates a folder inside the selected folder and starts renaming', async () => {
    const { store, spy } = await setup();
    const id = await store.createFolder('api', 'New Folder');
    expect(spy.create).toHaveBeenCalledWith(
      expect.objectContaining({ parentId: 'api', name: 'New Folder', data: { kind: 'folder' } }),
    );
    expect(store.renaming).toBe(id);
    expect(store.expanded).toContain('api');
  });

  it('deletes after confirmation and offers undo', async () => {
    const { store, spy } = await setup();
    vi.spyOn(dialogs, 'confirm').mockResolvedValue(true);
    const show = vi.spyOn(toasts, 'show');
    await store.remove(['api', 'a']);
    // Children of selected folders are not sent separately.
    expect(spy.delete).toHaveBeenCalledWith({ ids: ['api'] });
    const action = show.mock.calls.at(-1)![0].action!;
    action.run();
    await Promise.resolve();
    expect(spy.restore).toHaveBeenCalledWith({ ids: ['api'] });
    vi.restoreAllMocks();
  });

  it('does nothing when deletion is not confirmed', async () => {
    const { store, spy } = await setup();
    vi.spyOn(dialogs, 'confirm').mockResolvedValue(false);
    await store.remove(['a']);
    expect(spy.delete).not.toHaveBeenCalled();
    vi.restoreAllMocks();
  });

  it('cut + paste moves, copy + paste copies, into folders or next to requests', async () => {
    const { store, spy } = await setup();
    store.cut(['a']);
    await store.paste('root-req');
    expect(spy.move).toHaveBeenLastCalledWith({
      ids: ['a'],
      collectionId: 'c1',
      parentId: null,
      placement: { type: 'after', id: 'root-req' },
    });
    expect(store.clipboard).toBeNull();

    store.copy(['root-req']);
    await store.paste('api');
    expect(spy.copy).toHaveBeenLastCalledWith({
      ids: ['root-req'],
      collectionId: 'c1',
      parentId: 'api',
      placement: { type: 'end' },
    });
    expect(store.clipboard?.mode).toBe('copy');
  });

  it('copies cURL for selected requests', async () => {
    const { store } = await setup();
    const writeText = vi.fn();
    Object.assign(navigator, { clipboard: { writeText } });
    await store.copyAsCurl(['api', 'a', 'root-req']);
    expect(writeText).toHaveBeenCalledWith('curl a\n\ncurl root-req');
  });

  it('reloads on tree.changed for the collection', async () => {
    const nodes = sample();
    const { store, t } = await setup(nodes);
    nodes.push(n('z', null, 'request', 'zeta', 'z', 'GET'));
    t.emit({ topic: 'tree.changed', payload: { collectionIds: ['c1'], nodeIds: [] } });
    await new Promise((r) => setTimeout(r, 0));
    expect(store.items.map((i) => i.id)).toContain('z');
  });

  it('computes folder paths', async () => {
    const nodes = [
      ...sample(),
      n('deep', 'api', 'folder', 'v2', 'e'),
      n('x', 'deep', 'request', 'x', 'f', 'GET'),
    ];
    const { store } = await setup(nodes);
    expect(store.pathOf('x')).toEqual(['api', 'v2']);
    expect(store.pathOf('root-req')).toEqual([]);
  });
});
