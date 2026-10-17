import { describe, expect, it, vi } from 'vitest';
import { HotkeyRegistry, comboFromEvent, formatCombo, matches, parseCombo } from './hotkeys';

const key = (init: KeyboardEventInit) => new KeyboardEvent('keydown', init);

describe('parseCombo', () => {
  it('normalizes modifiers and keys to codes', () => {
    expect(parseCombo('Mod+Shift+K', 'linux')).toEqual({
      code: 'KeyK',
      ctrl: true,
      meta: false,
      alt: false,
      shift: true,
    });
    expect(parseCombo('Mod+Shift+K', 'mac')).toEqual({
      code: 'KeyK',
      ctrl: false,
      meta: true,
      alt: false,
      shift: true,
    });
    expect(parseCombo('Ctrl+Tab', 'mac').ctrl).toBe(true);
    expect(parseCombo('Mod+,', 'linux').code).toBe('Comma');
    expect(parseCombo('Mod+1', 'linux').code).toBe('Digit1');
    expect(parseCombo('F2', 'linux').code).toBe('F2');
    expect(parseCombo('Esc', 'linux').code).toBe('Escape');
    expect(parseCombo('Del', 'linux').code).toBe('Delete');
    expect(parseCombo('Alt+Mod+L', 'linux')).toMatchObject({ alt: true, ctrl: true, code: 'KeyL' });
  });

  it('rejects malformed combos', () => {
    expect(() => parseCombo('Mod+', 'linux')).toThrow();
    expect(() => parseCombo('Hyper+K', 'linux')).toThrow();
    expect(() => parseCombo('Mod+K+J', 'linux')).toThrow();
  });
});

describe('matches', () => {
  it('uses physical key codes so non-latin layouts work', () => {
    // Ctrl+S on a Russian layout produces key "ы" but code "KeyS".
    expect(
      matches(key({ key: 'ы', code: 'KeyS', ctrlKey: true }), parseCombo('Mod+S', 'linux')),
    ).toBe(true);
  });

  it('requires exact modifiers', () => {
    const combo = parseCombo('Mod+S', 'linux');
    expect(matches(key({ code: 'KeyS', ctrlKey: true, shiftKey: true }), combo)).toBe(false);
    expect(matches(key({ code: 'KeyS' }), combo)).toBe(false);
  });
});

describe('formatCombo', () => {
  it('formats per platform', () => {
    expect(formatCombo('Mod+Shift+K', 'linux')).toBe('Ctrl+Shift+K');
    expect(formatCombo('Mod+Shift+K', 'mac')).toBe('⇧⌘K');
    expect(formatCombo('Mod+Enter', 'windows')).toBe('Ctrl+Enter');
    expect(formatCombo('Mod+,', 'mac')).toBe('⌘,');
  });
});

describe('comboFromEvent', () => {
  it('records a canonical combo', () => {
    expect(
      comboFromEvent(key({ key: 'K', code: 'KeyK', ctrlKey: true, shiftKey: true }), 'linux'),
    ).toBe('Mod+Shift+K');
    expect(comboFromEvent(key({ key: 'k', code: 'KeyK', metaKey: true }), 'mac')).toBe('Mod+K');
    expect(comboFromEvent(key({ key: 'Tab', code: 'Tab', ctrlKey: true }), 'mac')).toBe('Ctrl+Tab');
  });

  it('ignores lone modifiers', () => {
    expect(
      comboFromEvent(key({ key: 'Shift', code: 'ShiftLeft', shiftKey: true }), 'linux'),
    ).toBeNull();
  });
});

describe('HotkeyRegistry', () => {
  function registry() {
    const r = new HotkeyRegistry('linux');
    r.define({ id: 'request.send', combos: ['Mod+Enter'], scope: 'global' });
    r.define({ id: 'tree.rename', combos: ['F2'], scope: 'tree' });
    r.define({ id: 'tab.close', combos: ['Mod+W'], scope: 'global' });
    r.define({ id: 'tree.delete', combos: ['Del'], scope: 'tree' });
    return r;
  }

  it('resolves actions honoring active scopes', () => {
    const r = registry();
    expect(r.resolve(key({ code: 'Enter', ctrlKey: true }), ['global'])).toBe('request.send');
    expect(r.resolve(key({ code: 'F2' }), ['global'])).toBeNull();
    expect(r.resolve(key({ code: 'F2' }), ['global', 'tree'])).toBe('tree.rename');
  });

  it('prefers the most specific scope', () => {
    const r = registry();
    r.define({ id: 'editor.save', combos: ['Mod+W'], scope: 'editor' });
    expect(r.resolve(key({ code: 'KeyW', ctrlKey: true }), ['global', 'editor'])).toBe(
      'editor.save',
    );
  });

  it('applies user overrides and can reset them', () => {
    const r = registry();
    r.setOverride('request.send', ['Mod+R']);
    expect(r.resolve(key({ code: 'Enter', ctrlKey: true }), ['global'])).toBeNull();
    expect(r.resolve(key({ code: 'KeyR', ctrlKey: true }), ['global'])).toBe('request.send');
    r.resetOverride('request.send');
    expect(r.combosFor('request.send')).toEqual(['Mod+Enter']);
  });

  it('reports conflicts within the same scope only', () => {
    const r = registry();
    r.setOverride('tab.close', ['Mod+Enter']);
    r.define({ id: 'tree.open', combos: ['Mod+Enter'], scope: 'tree' });
    expect(r.conflicts()).toEqual([
      { combo: 'Mod+Enter', scope: 'global', actions: ['request.send', 'tab.close'] },
    ]);
  });

  it('dispatches handlers and prevents default', () => {
    const r = registry();
    const send = vi.fn();
    r.on('request.send', send);
    const event = key({ code: 'Enter', ctrlKey: true, cancelable: true });
    expect(r.handle(event, ['global'])).toBe(true);
    expect(send).toHaveBeenCalledOnce();
    expect(event.defaultPrevented).toBe(true);
  });

  it('does not swallow keys without a handler', () => {
    const r = registry();
    const event = key({ code: 'KeyW', ctrlKey: true, cancelable: true });
    expect(r.handle(event, ['global'])).toBe(false);
    expect(event.defaultPrevented).toBe(false);
  });
});
