import { describe, expect, it } from 'vitest';
import { FakeTransport } from './testing/fakeTransport';
import { DEFAULT_PREFERENCES, PREFERENCES_KEY, PreferencesStore } from './preferences.svelte';

describe('PreferencesStore', () => {
  it('starts with defaults and merges stored values', async () => {
    const transport = new FakeTransport();
    transport.withSettings({ [PREFERENCES_KEY]: { theme: 'dark', bogus: 1 } });
    const store = new PreferencesStore(transport);
    expect(store.value).toEqual(DEFAULT_PREFERENCES);
    await store.load();
    expect(store.value.theme).toBe('dark');
    expect(store.value.sidebarWidth).toBe(DEFAULT_PREFERENCES.sidebarWidth);
    expect('bogus' in store.value).toBe(false);
  });

  it('persists updates', async () => {
    const transport = new FakeTransport();
    const saved = transport.withSettings();
    const store = new PreferencesStore(transport);
    await store.load();
    await store.update({ sidebarCollapsed: true });
    expect(store.value.sidebarCollapsed).toBe(true);
    expect(saved.get(PREFERENCES_KEY)).toMatchObject({ sidebarCollapsed: true });
  });

  it('applies changes made by other windows', async () => {
    const transport = new FakeTransport();
    transport.withSettings();
    const store = new PreferencesStore(transport);
    await store.load();
    transport.emit({
      topic: 'settings.changed',
      payload: { key: PREFERENCES_KEY, value: { theme: 'light' } },
    });
    expect(store.value.theme).toBe('light');
    transport.emit({
      topic: 'settings.changed',
      payload: { key: 'other', value: { theme: 'dark' } },
    });
    expect(store.value.theme).toBe('light');
  });

  it('ignores invalid stored values', async () => {
    const transport = new FakeTransport();
    transport.withSettings({
      [PREFERENCES_KEY]: { theme: 5, responseLayout: 'diagonal', sidebarWidth: 'wide' },
    });
    const store = new PreferencesStore(transport);
    await store.load();
    expect(store.value).toEqual(DEFAULT_PREFERENCES);
  });
});
