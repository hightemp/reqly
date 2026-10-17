// Application-wide services, created once at startup and passed to the shell.

import type { Transport } from './api/transport';
import { DEFAULT_ACTIONS } from './hotkeys/actions';
import { HotkeyRegistry, detectPlatform, type Platform } from './hotkeys/hotkeys';
import { i18n } from './i18n';
import { PreferencesStore } from './preferences.svelte';
import { ThemeManager, type MediaQuery } from './themes/manager.svelte';

export interface Services {
  transport: Transport;
  prefs: PreferencesStore;
  themes: ThemeManager;
  hotkeys: HotkeyRegistry;
  platform: Platform;
}

export interface CreateServicesOptions {
  platform?: Platform;
  media?: MediaQuery;
  root?: HTMLElement;
}

export async function createServices(
  transport: Transport,
  options: CreateServicesOptions = {},
): Promise<Services> {
  const platform = options.platform ?? detectPlatform();
  const prefs = new PreferencesStore(transport);
  await prefs.load().catch((e: unknown) => console.error('[prefs] load failed', e));

  const themes = new ThemeManager({ media: options.media, root: options.root });
  const hotkeys = new HotkeyRegistry(platform);
  DEFAULT_ACTIONS.forEach((a) => hotkeys.define(a));

  return { transport, prefs, themes, hotkeys, platform };
}

/** Keeps theme and locale in sync with preferences. Call inside a component. */
export function bindPreferences(services: Services) {
  $effect(() => services.themes.setPreference(services.prefs.value.theme));
  $effect(() => {
    const locale = services.prefs.value.locale;
    if (locale) i18n.setLocale(locale);
  });
}
