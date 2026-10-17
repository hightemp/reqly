import { describe, expect, it } from 'vitest';
import { DEFAULT_ACTIONS } from './actions';
import { HotkeyRegistry } from './hotkeys';

describe('default actions', () => {
  it.each(['linux', 'mac', 'windows'] as const)('have no conflicts on %s', (platform) => {
    const r = new HotkeyRegistry(platform);
    DEFAULT_ACTIONS.forEach((a) => r.define(a));
    expect(r.conflicts()).toEqual([]);
  });
});
