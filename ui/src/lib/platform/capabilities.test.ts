import { describe, expect, it } from 'vitest';
import { FakeTransport } from '$lib/testing/fakeTransport';
import { Capabilities } from './capabilities.svelte';

describe('Capabilities', () => {
  it('loads what the platform cannot do from app.info', async () => {
    const transport = new FakeTransport().handle('app.info', () => ({
      name: 'Reqly',
      version: '0',
      platform: 'mobile',
      dataDir: null,
      unavailable: ['unixSocket', 'osTag'],
    }));
    const c = new Capabilities();
    expect(c.has('unixSocket')).toBe(true);
    await c.load(transport);
    expect(c.mobile).toBe(true);
    expect(c.has('unixSocket')).toBe(false);
    expect(c.has('interface')).toBe(true);
  });
});
