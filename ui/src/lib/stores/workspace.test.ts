import { describe, expect, it } from 'vitest';
import { PreferencesStore } from '$lib/preferences.svelte';
import { FakeTransport } from '$lib/testing/fakeTransport';
import { WorkspaceStore } from './workspace.svelte';

const ws = (id: string, name: string) => ({
  id,
  name,
  createdAt: 0,
  updatedAt: 0,
  version: 1,
  deletedAt: null,
  sortKey: 'V',
});
const col = (id: string, workspaceId: string, name: string) => ({
  ...ws(id, name),
  workspaceId,
  settings: {},
});

function backend() {
  const t = new FakeTransport();
  t.withSettings();
  const workspaces = [ws('w1', 'Main'), ws('w2', 'Side')];
  const collections = [col('c1', 'w1', 'API'), col('c2', 'w1', 'Admin'), col('c3', 'w2', 'Other')];
  t.handle('workspace.ensureDefault', () => workspaces as never);
  t.handle(
    'collection.list',
    ({ workspaceId }) => collections.filter((c) => c.workspaceId === workspaceId) as never,
  );
  t.handle('collection.create', ({ workspaceId, name }) => {
    const c = col(`c${collections.length + 1}`, workspaceId, name);
    collections.push(c);
    return c as never;
  });
  return { t, collections };
}

async function setup() {
  const { t, collections } = backend();
  const prefs = new PreferencesStore(t);
  await prefs.load();
  const store = new WorkspaceStore(t, prefs);
  await store.load();
  return { t, prefs, store, collections };
}

describe('WorkspaceStore', () => {
  it('loads and selects the first workspace and collection', async () => {
    const { store } = await setup();
    expect(store.workspaces.map((w) => w.name)).toEqual(['Main', 'Side']);
    expect(store.activeWorkspace?.id).toBe('w1');
    expect(store.collections.map((c) => c.name)).toEqual(['API', 'Admin']);
    expect(store.activeCollection?.id).toBe('c1');
  });

  it('restores the last active ids from preferences', async () => {
    const { t, prefs } = await setup();
    await prefs.update({ activeWorkspaceId: 'w2', activeCollectionId: 'c3' });
    const store = new WorkspaceStore(t, prefs);
    await store.load();
    expect(store.activeWorkspace?.id).toBe('w2');
    expect(store.activeCollection?.id).toBe('c3');
  });

  it('switching workspace loads its collections and remembers the choice', async () => {
    const { store, prefs } = await setup();
    await store.selectWorkspace('w2');
    expect(store.collections.map((c) => c.id)).toEqual(['c3']);
    expect(store.activeCollection?.id).toBe('c3');
    expect(prefs.value.activeWorkspaceId).toBe('w2');
    expect(prefs.value.activeCollectionId).toBe('c3');
  });

  it('reloads collections on collections.changed for the active workspace', async () => {
    const { t, store, collections } = await setup();
    collections.push(col('c9', 'w1', 'New'));
    t.emit({ topic: 'collections.changed', payload: { workspaceId: 'w1' } });
    await new Promise((r) => setTimeout(r, 0));
    expect(store.collections.map((c) => c.id)).toContain('c9');
  });

  it('creates a collection and selects it', async () => {
    const { store } = await setup();
    await store.createCollection('Billing');
    expect(store.activeCollection?.name).toBe('Billing');
  });
});
