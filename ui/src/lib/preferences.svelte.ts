// UI preferences persisted through the core `settings.*` commands, so the
// desktop app and every browser tab of the server share them.

import type { Transport } from './api/transport';
import { LOCALES, type Locale } from './i18n';

export const PREFERENCES_KEY = 'ui.preferences';

export interface Preferences {
  /** Theme id or `system`. */
  theme: string;
  /** `null` follows the OS / browser language. */
  locale: Locale | null;
  sidebarWidth: number;
  sidebarCollapsed: boolean;
  /** Where the response panel sits relative to the request editor. */
  responseLayout: 'right' | 'bottom';
  requestPaneSize: number;
  /** `tree` (folders and requests) or `categories` (folder tree + request list). */
  sidebarMode: 'tree' | 'categories';
  /** Height of the category tree in `categories` mode. */
  categoriesHeight: number;
  /** Request list in `categories` mode without subfolder groups. */
  flatList: boolean;
  autosave: boolean;
  activeWorkspaceId: string | null;
  activeCollectionId: string | null;
}

export const DEFAULT_PREFERENCES: Preferences = {
  theme: 'system',
  locale: null,
  sidebarWidth: 280,
  sidebarCollapsed: false,
  responseLayout: 'right',
  requestPaneSize: 520,
  sidebarMode: 'tree',
  categoriesHeight: 240,
  flatList: false,
  autosave: true,
  activeWorkspaceId: null,
  activeCollectionId: null,
};

const validators: { [K in keyof Preferences]: (v: unknown) => boolean } = {
  theme: (v) => typeof v === 'string' && v.length > 0,
  locale: (v) => v === null || LOCALES.includes(v as Locale),
  sidebarWidth: (v) => typeof v === 'number' && v > 0,
  sidebarCollapsed: (v) => typeof v === 'boolean',
  responseLayout: (v) => v === 'right' || v === 'bottom',
  requestPaneSize: (v) => typeof v === 'number' && v > 0,
  sidebarMode: (v) => v === 'tree' || v === 'categories',
  categoriesHeight: (v) => typeof v === 'number' && v > 0,
  flatList: (v) => typeof v === 'boolean',
  autosave: (v) => typeof v === 'boolean',
  activeWorkspaceId: (v) => v === null || typeof v === 'string',
  activeCollectionId: (v) => v === null || typeof v === 'string',
};

/** Keeps known, valid fields of untrusted stored data on top of defaults. */
export function sanitize(raw: unknown): Preferences {
  const result = { ...DEFAULT_PREFERENCES };
  if (!raw || typeof raw !== 'object') return result;
  for (const key of Object.keys(validators) as (keyof Preferences)[]) {
    const value = (raw as Record<string, unknown>)[key];
    if (value !== undefined && validators[key](value))
      (result as Record<string, unknown>)[key] = value;
  }
  return result;
}

export class PreferencesStore {
  value = $state<Preferences>({ ...DEFAULT_PREFERENCES });
  private unsubscribe: (() => void) | null = null;

  constructor(private readonly transport: Transport) {}

  async load() {
    const { value } = await this.transport.call('settings.get', { key: PREFERENCES_KEY });
    this.value = sanitize(value);
    this.unsubscribe ??= this.transport.subscribe((event) => {
      const payload = event.payload as { key?: string; value?: unknown } | null;
      if (event.topic === 'settings.changed' && payload?.key === PREFERENCES_KEY) {
        this.value = sanitize(payload.value);
      }
    });
  }

  async update(patch: Partial<Preferences>) {
    this.value = { ...this.value, ...patch };
    await this.transport.call('settings.set', { key: PREFERENCES_KEY, value: this.value });
  }

  destroy() {
    this.unsubscribe?.();
    this.unsubscribe = null;
  }
}
