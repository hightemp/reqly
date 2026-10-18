import { describe, expect, it } from 'vitest';
import { FakeTransport } from '$lib/testing/fakeTransport';
import type { Role } from '$lib/api/generated/Role';
import { AccountStore } from './account.svelte';

const user = {
  id: 'u1',
  username: 'ann',
  displayName: 'Ann',
  isAdmin: false,
  disabled: false,
  createdAt: 0,
};

describe('AccountStore', () => {
  it('allows everything without a user', async () => {
    const transport = new FakeTransport();
    transport.handle('account.me', () => ({ user: null, roles: {} }));
    const account = new AccountStore(transport);
    await account.load();
    expect(account.user).toBeNull();
    expect(account.canEdit('w1')).toBe(true);
    expect(account.canManage(null)).toBe(true);
  });

  it('follows workspace roles of a user', async () => {
    const transport = new FakeTransport();
    transport.handle('account.me', () => ({ user, roles: { w1: 'viewer', w2: 'editor' } }));
    const account = new AccountStore(transport);
    await account.load();
    expect(account.canEdit('w1')).toBe(false);
    expect(account.canEdit('w2')).toBe(true);
    expect(account.canManage('w2')).toBe(false);
    expect(account.role('w3')).toBeNull();
  });

  it('treats administrators as owners', async () => {
    const transport = new FakeTransport();
    transport.handle('account.me', () => ({ user: { ...user, isAdmin: true }, roles: {} }));
    const account = new AccountStore(transport);
    await account.load();
    expect(account.canManage('anything')).toBe(true);
  });

  it('reloads roles when workspaces change', async () => {
    const transport = new FakeTransport();
    let roles: Record<string, Role> = { w1: 'viewer' };
    transport.handle('account.me', () => ({ user, roles }));
    const account = new AccountStore(transport);
    await account.load();
    roles = { w1: 'owner' };
    transport.emit({ topic: 'workspaces.changed', payload: {} });
    await new Promise((r) => setTimeout(r, 0));
    expect(account.canManage('w1')).toBe(true);
  });
});
