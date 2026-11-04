import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';
import type { SyncConflict } from '$lib/api/generated/SyncConflict';
import type { SyncStatus } from '$lib/api/generated/SyncStatus';
import { SyncStore } from '$lib/stores/sync.svelte';
import { FakeTransport } from '$lib/testing/fakeTransport';
import SyncConflictsDialog from './SyncConflictsDialog.svelte';
import SyncSettings from './SyncSettings.svelte';

const off: SyncStatus = {
  url: null,
  user: null,
  secrets: false,
  workspaces: [],
  syncing: false,
  lastSyncAt: null,
  lastError: null,
  pending: 0,
  conflicts: 0,
};

const conflict: SyncConflict = {
  local: {
    kind: 'node',
    id: 'n1',
    workspaceId: 'w1',
    deleted: false,
    fields: { name: 'List users' },
  },
  remote: {
    kind: 'node',
    id: 'n1',
    workspaceId: 'w1',
    deleted: true,
    fields: { name: 'List users' },
  },
};

function setup(initial: SyncStatus) {
  const transport = new FakeTransport();
  let status = initial;
  let conflicts = initial.conflicts ? [conflict] : [];
  transport.handle('sync.status', () => status);
  transport.handle('sync.conflicts', () => conflicts);
  transport.handle('sync.log', () => [
    { at: 1000, pulled: 3, pushed: 1, conflicts: 0, rejected: 0, error: null },
  ]);
  transport.handle('sync.workspaces', () => ({
    workspaces: [
      {
        id: 'w1',
        name: 'Shared',
        local: true,
        remote: true,
        synced: status.workspaces.includes('w1'),
      },
      {
        id: 'w2',
        name: 'Team',
        local: false,
        remote: true,
        synced: status.workspaces.includes('w2'),
      },
    ],
    error: null,
  }));
  transport.handle('sync.connect', (input) => {
    status = { ...off, url: input.url, user: 'Ada (ada)', lastSyncAt: 1000 };
    return status;
  });
  transport.handle('sync.follow', (input) => {
    status = { ...status, workspaces: [...status.workspaces, input.workspaceId] };
    return status;
  });
  transport.handle('sync.resolve', () => {
    conflicts = [];
    status = { ...status, conflicts: 0 };
    return status;
  });
  const sync = new SyncStore(transport);
  return { transport, sync };
}

describe('SyncSettings', () => {
  it('connects with a user name and password', async () => {
    const { transport, sync } = setup(off);
    render(SyncSettings, { sync });
    await fireEvent.input(await screen.findByLabelText('Server URL'), {
      target: { value: 'https://reqly.test' },
    });
    await fireEvent.input(screen.getByLabelText('User name'), { target: { value: 'ada' } });
    await fireEvent.input(screen.getByLabelText('Password'), { target: { value: 'secret' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Connect' }));
    await screen.findByText('Connected to https://reqly.test as Ada (ada)');
    expect(transport.calls.find((c) => c.name === 'sync.connect')?.input).toEqual({
      url: 'https://reqly.test',
      username: 'ada',
      password: 'secret',
    });
    // Workspaces of the device and the server, to pick from.
    await fireEvent.click(await screen.findByRole('checkbox', { name: 'Sync Team' }));
    await waitFor(() =>
      expect(transport.calls.find((c) => c.name === 'sync.follow')?.input).toEqual({
        workspaceId: 'w2',
      }),
    );
    expect(sync.status?.workspaces).toEqual(['w2']);
  });

  it('shows the state and the log when connected', async () => {
    const { sync } = setup({
      ...off,
      url: 'https://reqly.test',
      workspaces: ['w1'],
      lastSyncAt: 1000,
      pending: 2,
    });
    render(SyncSettings, { sync });
    await screen.findByText('Connected to https://reqly.test');
    expect(screen.getByText('2 changes not sent yet')).toBeTruthy();
    expect(await screen.findByText('3 received, 1 sent')).toBeTruthy();
    expect(screen.getByRole('checkbox', { name: 'Sync Shared' }).getAttribute('aria-checked')).toBe(
      'true',
    );
  });

  it('settles conflicts', async () => {
    const { transport, sync } = setup({ ...off, url: 'https://reqly.test', conflicts: 1 });
    await sync.load();
    sync.conflictsOpen = true;
    render(SyncConflictsDialog, { sync });
    expect(screen.getByText('List users')).toBeTruthy();
    expect(screen.getByText(/Deleted on the server, changed here/)).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Keep mine' }));
    await waitFor(() => expect(sync.conflictsOpen).toBe(false));
    expect(transport.calls.find((c) => c.name === 'sync.resolve')?.input).toEqual({
      kind: 'node',
      id: 'n1',
      resolution: 'keepLocal',
    });
  });
});
