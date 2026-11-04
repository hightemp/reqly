import { describe, expect, it } from 'vitest';
import type { SyncConflict } from '$lib/api/generated/SyncConflict';
import type { SyncStatus } from '$lib/api/generated/SyncStatus';
import { FakeTransport } from '$lib/testing/fakeTransport';
import { SyncStore } from './sync.svelte';

const status = (over: Partial<SyncStatus> = {}): SyncStatus => ({
  url: 'https://reqly.test',
  user: 'Ada (ada)',
  secrets: false,
  workspaces: ['w1'],
  syncing: false,
  lastSyncAt: 1000,
  lastError: null,
  pending: 0,
  conflicts: 0,
  ...over,
});

const conflict: SyncConflict = {
  local: { kind: 'node', id: 'n1', workspaceId: 'w1', deleted: false, fields: { name: 'Users' } },
  remote: { kind: 'node', id: 'n1', workspaceId: 'w1', deleted: true, fields: { name: 'Users' } },
};

function setup(initial: SyncStatus) {
  const transport = new FakeTransport();
  let current = initial;
  transport.handle('sync.status', () => current);
  transport.handle('sync.conflicts', () => (current.conflicts ? [conflict] : []));
  transport.handle('sync.now', () => current);
  transport.handle('sync.log', () => []);
  transport.handle('sync.workspaces', () => ({
    workspaces: [
      { id: 'w1', name: 'Shared', local: true, remote: true, synced: true },
      { id: 'w2', name: 'Team', local: false, remote: true, synced: false },
    ],
    error: null,
  }));
  transport.handle('sync.follow', (input) => {
    current = { ...current, workspaces: [...current.workspaces, input.workspaceId] };
    return current;
  });
  transport.handle('sync.unfollow', (input) => {
    current = { ...current, workspaces: current.workspaces.filter((w) => w !== input.workspaceId) };
    return current;
  });
  transport.handle('sync.resolve', () => {
    current = { ...current, conflicts: 0 };
    return current;
  });
  transport.handle('sync.connect', (input) => {
    if (input.password === 'wrong') throw { code: 'auth.invalid', message: 'x', params: {} };
    current = status({ url: input.url, workspaces: [] });
    return current;
  });
  transport.handle('sync.disconnect', () => {
    current = status({ url: null, user: null, workspaces: [] });
    return current;
  });
  const store = new SyncStore(transport);
  return { transport, store, set: (s: SyncStatus) => (current = s) };
}

describe('SyncStore', () => {
  it('is off without a server', async () => {
    const { store } = setup(status({ url: null, user: null, workspaces: [], lastSyncAt: null }));
    await store.load();
    expect(store.connected).toBe(false);
    expect(store.state).toBe('off');
  });

  it('sums up the state for the status bar', async () => {
    const { store, set } = setup(status());
    await store.load();
    expect(store.state).toBe('ok');
    set(status({ pending: 2 }));
    await store.load();
    expect(store.state).toBe('pending');
    set(status({ syncing: true }));
    await store.load();
    expect(store.state).toBe('syncing');
    set(status({ lastError: { code: 'sync.network', message: 'down', params: {} } }));
    await store.load();
    expect(store.state).toBe('error');
    set(status({ conflicts: 1 }));
    await store.load();
    expect(store.state).toBe('conflicts');
    expect(store.conflicts).toEqual([conflict]);
  });

  it('follows status events', async () => {
    const { transport, store } = setup(status());
    await store.load();
    transport.emit({ topic: 'sync.status', payload: status({ syncing: true }) });
    expect(store.state).toBe('syncing');
  });

  it('connects and reports errors', async () => {
    const { store } = setup(status({ url: null, user: null, workspaces: [] }));
    await store.load();
    expect(
      await store.connect({ url: 'https://reqly.test', username: 'ada', password: 'wrong' }),
    ).toBe(false);
    expect(store.error).toBeTruthy();
    expect(
      await store.connect({ url: 'https://reqly.test', username: 'ada', password: 'ok' }),
    ).toBe(true);
    expect(store.error).toBeNull();
    expect(store.connected).toBe(true);
    await store.disconnect();
    expect(store.connected).toBe(false);
  });

  it('picks workspaces to sync', async () => {
    const { transport, store } = setup(status());
    await store.load();
    await store.loadWorkspaces();
    expect(store.workspaces.map((w) => w.name)).toEqual(['Shared', 'Team']);
    await store.setSynced('w2', true);
    expect(store.status?.workspaces).toEqual(['w1', 'w2']);
    await store.setSynced('w1', false);
    expect(store.status?.workspaces).toEqual(['w2']);
    expect(transport.calls.filter((c) => c.name === 'sync.workspaces')).toHaveLength(3);
  });

  it('resolves conflicts', async () => {
    const { transport, store } = setup(status({ conflicts: 1 }));
    await store.load();
    await store.resolve(conflict, 'keepLocal');
    expect(transport.calls.find((c) => c.name === 'sync.resolve')?.input).toEqual({
      kind: 'node',
      id: 'n1',
      resolution: 'keepLocal',
    });
    expect(store.conflicts).toEqual([]);
    expect(store.state).toBe('ok');
  });
});
