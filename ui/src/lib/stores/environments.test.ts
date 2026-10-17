import { describe, expect, it } from 'vitest';
import type { Environment } from '$lib/api/generated/Environment';
import { FakeTransport } from '$lib/testing/fakeTransport';
import { EnvironmentStore } from './environments.svelte';

const env = (id: string, kind: Environment['kind'], name: string, data = {}): Environment => ({
  id,
  kind,
  name,
  data,
  workspaceId: 'w1',
  collectionId: kind === 'global' ? null : 'c1',
  color: null,
  secrets: [],
  createdAt: 0,
  updatedAt: 0,
  version: 1,
  deletedAt: null,
  sortKey: 'V',
});

function setup() {
  const t = new FakeTransport();
  const state = {
    collection: [env('b', 'base', 'Base'), env('d', 'sub', 'dev')],
    global: [env('g', 'global', 'shared')],
    activeId: null as string | null,
    activeGlobalId: null as string | null,
  };
  t.handle('environment.list', () => ({ ...state }));
  t.handle('environment.setActive', ({ environmentId }) => {
    state.activeId = environmentId;
    return {};
  });
  t.handle('environment.setActiveGlobal', ({ environmentId }) => {
    state.activeGlobalId = environmentId;
    return {};
  });
  t.handle('environment.create', ({ kind, name }) => {
    const e = env(`n${state.collection.length}`, kind, name);
    (kind === 'global' ? state.global : state.collection).push(e);
    return e;
  });
  t.handle('environment.update', ({ id, name }) => {
    const e = state.collection.find((x) => x.id === id)!;
    if (name) e.name = name;
    return { ...e };
  });
  let variableCalls = 0;
  t.handle('template.variables', () => {
    variableCalls++;
    return [];
  });
  return { t, state, store: new EnvironmentStore(t), variableCalls: () => variableCalls };
}

describe('EnvironmentStore', () => {
  it('loads base, sub and global environments', async () => {
    const { store } = setup();
    await store.load('c1');
    expect(store.base?.name).toBe('Base');
    expect(store.subs.map((e) => e.name)).toEqual(['dev']);
    expect(store.global.map((e) => e.name)).toEqual(['shared']);
    expect(store.workspaceId).toBe('w1');
  });

  it('activates environments', async () => {
    const { store, state } = setup();
    await store.load('c1');
    await store.setActive('d');
    await store.setActiveGlobal('g');
    expect(state.activeId).toBe('d');
    expect(store.active?.name).toBe('dev');
    expect(store.activeGlobal?.name).toBe('shared');
  });

  it('creates and updates', async () => {
    const { store } = setup();
    await store.load('c1');
    await store.create('sub', 'prod');
    expect(store.subs.map((e) => e.name)).toEqual(['dev', 'prod']);
    await store.update('d', { name: 'development' });
    expect(store.subs[0]!.name).toBe('development');
  });

  it('caches variables until something changes', async () => {
    const { t, store, variableCalls } = setup();
    await store.load('c1');
    await store.variables('r1');
    await store.variables('r1');
    expect(variableCalls()).toBe(1);
    const rev = store.revision;
    t.emit({ topic: 'tree.changed', payload: { collectionIds: ['c1'], nodeIds: [] } });
    expect(store.revision).toBe(rev + 1);
    await store.variables('r1');
    expect(variableCalls()).toBe(2);
  });

  it('reloads on environment events of its collection', async () => {
    const { t, store, state } = setup();
    await store.load('c1');
    state.collection.push(env('p', 'sub', 'prod'));
    t.emit({ topic: 'environments.changed', payload: { workspaceId: 'w1', collectionId: 'c1' } });
    await new Promise((r) => setTimeout(r, 0));
    expect(store.subs).toHaveLength(2);
  });
});
