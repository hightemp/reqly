import { describe, expect, it, vi } from 'vitest';
import { ThemeManager, type MediaQuery } from './manager.svelte';
import { builtinThemes } from './builtin';

function fakeMedia(dark: boolean) {
  const listeners = new Set<() => void>();
  const media: MediaQuery & { set(v: boolean): void } = {
    get matches() {
      return dark;
    },
    addEventListener: (_: 'change', l: () => void) => listeners.add(l),
    removeEventListener: (_: 'change', l: () => void) => listeners.delete(l),
    set(v: boolean) {
      dark = v;
      listeners.forEach((l) => l());
    },
  };
  return media;
}

describe('ThemeManager', () => {
  it('follows the OS in system mode', () => {
    const media = fakeMedia(true);
    const el = document.createElement('div');
    const m = new ThemeManager({ media, root: el });
    m.setPreference('system');
    expect(m.active.id).toBe('dark');
    expect(el.dataset.theme).toBe('dark');
    media.set(false);
    expect(m.active.id).toBe('light');
    expect(el.dataset.theme).toBe('light');
  });

  it('uses an explicit theme regardless of OS', () => {
    const media = fakeMedia(true);
    const m = new ThemeManager({ media, root: document.createElement('div') });
    m.setPreference('light');
    media.set(true);
    expect(m.active.id).toBe('light');
  });

  it('registers custom themes extending builtins', () => {
    const m = new ThemeManager({ media: fakeMedia(false), root: document.createElement('div') });
    m.addCustom({
      id: 'pink',
      name: 'Pink',
      kind: 'dark',
      extends: 'dark',
      colors: { accent: '#ff0080' },
    });
    m.setPreference('pink');
    expect(m.active.colors.accent).toBe('#ff0080');
    expect(m.active.colors.bg).toBe(builtinThemes.find((t) => t.id === 'dark')!.colors.bg);
    expect(m.themes.map((t) => t.id)).toContain('pink');
  });

  it('falls back to system when the preferred theme is unknown', () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    const m = new ThemeManager({ media: fakeMedia(false), root: document.createElement('div') });
    m.setPreference('missing');
    expect(m.active.id).toBe('light');
    warn.mockRestore();
  });

  it('refuses custom themes that clash with builtin ids', () => {
    const m = new ThemeManager({ media: fakeMedia(false), root: document.createElement('div') });
    expect(() =>
      m.addCustom({ id: 'dark', name: 'X', kind: 'dark', extends: 'light', colors: {} }),
    ).toThrow();
  });

  it('removes the OS listener on destroy', () => {
    const media = fakeMedia(false);
    const remove = vi.spyOn(media, 'removeEventListener');
    const m = new ThemeManager({ media, root: document.createElement('div') });
    m.destroy();
    expect(remove).toHaveBeenCalled();
  });
});
