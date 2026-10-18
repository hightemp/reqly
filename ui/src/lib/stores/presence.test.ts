import { describe, expect, it } from 'vitest';
import type { PresencePerson } from '$lib/api/generated/PresencePerson';
import { FakeTransport } from '$lib/testing/fakeTransport';
import { AccountStore } from './account.svelte';
import { PresenceStore } from './presence.svelte';

const me = {
  id: 'u1',
  username: 'ann',
  displayName: 'Ann',
  isAdmin: false,
  disabled: false,
  createdAt: 0,
};
const person = (
  clientId: string,
  userId: string,
  name: string,
  nodeId: string | null = null,
): PresencePerson => ({
  clientId,
  userId,
  name,
  nodeId,
});

async function setup() {
  const transport = Object.assign(new FakeTransport(), { clientId: 'tab-1' });
  transport.handle('account.me', () => ({ user: me, roles: {} }));
  const sent: unknown[] = [];
  transport.handle('presence.set', (input) => {
    sent.push(input);
    return [
      person('tab-1', 'u1', 'Ann'),
      person('tab-9', 'u2', 'Bob', 'r1'),
      person('tab-8', 'u2', 'Bob', 'r2'),
    ];
  });
  const account = new AccountStore(transport);
  await account.load();
  return { transport, sent, presence: new PresenceStore(transport, account) };
}

describe('PresenceStore', () => {
  it('reports the tab and lists the others', async () => {
    const { presence, sent } = await setup();
    presence.report('w1', 'r1');
    await new Promise((r) => setTimeout(r, 0));
    expect(sent).toEqual([{ clientId: 'tab-1', workspaceId: 'w1', nodeId: 'r1' }]);
    expect(presence.others.map((p) => p.name)).toEqual(['Bob']);
    expect(presence.on('r2').map((p) => p.name)).toEqual(['Bob']);
    expect(presence.on('r3')).toEqual([]);
  });

  it('follows changes of its workspace only', async () => {
    const { presence, transport } = await setup();
    presence.report('w1', null);
    await new Promise((r) => setTimeout(r, 0));
    transport.emit({ topic: 'presence.changed', payload: { workspaceId: 'w2', people: [] } });
    expect(presence.others).toHaveLength(1);
    transport.emit({ topic: 'presence.changed', payload: { workspaceId: 'w1', people: [] } });
    expect(presence.others).toHaveLength(0);
  });

  it('stays quiet without an account', async () => {
    const transport = new FakeTransport();
    transport.handle('account.me', () => ({ user: null, roles: {} }));
    const account = new AccountStore(transport);
    await account.load();
    new PresenceStore(transport, account).report('w1', null);
    expect(transport.calls.map((c) => c.name)).toEqual(['account.me']);
  });
});
