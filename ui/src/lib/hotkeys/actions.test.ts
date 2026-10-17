import { describe, expect, it } from 'vitest';
import { BROWSER_RESERVED, DEFAULT_ACTIONS, defaultActions } from './actions';
import { HotkeyRegistry, parseCombo } from './hotkeys';

describe('default actions', () => {
  it.each(['linux', 'mac', 'windows'] as const)('have no conflicts on %s', (platform) => {
    const r = new HotkeyRegistry(platform);
    DEFAULT_ACTIONS.forEach((a) => r.define(a));
    expect(r.conflicts()).toEqual([]);
  });
});

describe('web defaults', () => {
  it.each(['linux', 'mac', 'windows'] as const)(
    'avoid browser-reserved combos and conflicts on %s',
    (platform) => {
      const reserved = BROWSER_RESERVED.map((c) => JSON.stringify(parseCombo(c, platform)));
      const r = new HotkeyRegistry(platform);
      defaultActions('web').forEach((a) => r.define(a));
      expect(r.conflicts()).toEqual([]);
      for (const a of defaultActions('web')) {
        for (const c of a.combos)
          expect(reserved, `${a.id}: ${c}`).not.toContain(JSON.stringify(parseCombo(c, platform)));
      }
    },
  );
});
