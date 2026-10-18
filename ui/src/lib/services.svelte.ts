// Application-wide services, created once at startup and passed to the shell.

import type { Transport } from './api/transport';
import { defaultActions } from './hotkeys/actions';
import { HotkeyRegistry, detectPlatform, type Platform } from './hotkeys/hotkeys';
import { i18n } from './i18n';
import { PreferencesStore } from './preferences.svelte';
import { CollabService, userColor } from './collab/collab';
import { AccountStore } from './stores/account.svelte';
import { EditorStore } from './stores/editor.svelte';
import { EnvironmentStore } from './stores/environments.svelte';
import { ResponseStore } from './stores/responses.svelte';
import { StreamStore } from './stores/streams.svelte';
import { TabsStore } from './stores/tabs.svelte';
import { TreeStore } from './stores/tree.svelte';
import { WorkspaceStore } from './stores/workspace.svelte';
import { ThemeManager, type MediaQuery } from './themes/manager.svelte';
import { parseTheme } from './themes/theme';

/** Hotkey scopes active because of where keyboard focus is (e.g. `tree`). */
export class FocusScopes {
  active = $state<string[]>([]);

  set(scope: string, on: boolean) {
    const has = this.active.includes(scope);
    if (on && !has) this.active = [...this.active, scope];
    else if (!on && has) this.active = this.active.filter((s) => s !== scope);
  }

  get list(): string[] {
    return ['global', ...this.active];
  }
}

export interface Services {
  transport: Transport;
  prefs: PreferencesStore;
  themes: ThemeManager;
  hotkeys: HotkeyRegistry;
  platform: Platform;
  workspace: WorkspaceStore;
  tree: TreeStore;
  tabs: TabsStore;
  editor: EditorStore;
  responses: ResponseStore;
  streams: StreamStore;
  environments: EnvironmentStore;
  account: AccountStore;
  collab: CollabService;
  scopes: FocusScopes;
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
  defaultActions(transport.kind === 'tauri' ? 'desktop' : 'web').forEach((a) => hotkeys.define(a));

  await loadUserCustomizations(transport, themes, hotkeys);

  const account = new AccountStore(transport);
  await account.load().catch((e: unknown) => console.warn('[account] load failed', e));
  const collab = new CollabService(transport, () =>
    account.user ? { name: account.user.displayName, color: userColor(account.user.id) } : null,
  );
  const workspace = new WorkspaceStore(transport, prefs);
  const tree = new TreeStore(transport);
  const tabs = new TabsStore(transport);
  const editor = new EditorStore(
    transport,
    () => prefs.value.autosave,
    () => !account.canEdit(workspace.activeWorkspaceId),
    (id) => collab.paths(id),
  );
  const responses = new ResponseStore(transport, (id) => editor.flush(id));
  const streams = new StreamStore(transport, (id) => editor.flush(id));
  const environments = new EnvironmentStore(transport);
  try {
    await workspace.load();
    if (workspace.activeCollectionId) {
      await tree.load(workspace.activeCollectionId);
      await environments
        .load(workspace.activeCollectionId)
        .catch((e: unknown) => console.warn('[environments] load failed', e));
    }
    if (workspace.activeWorkspaceId) await tabs.load(workspace.activeWorkspaceId);
  } catch (e) {
    console.error('[workspace] load failed', e);
  }

  return {
    transport,
    prefs,
    themes,
    hotkeys,
    platform,
    workspace,
    tree,
    tabs,
    editor,
    responses,
    streams,
    environments,
    account,
    collab,
    scopes: new FocusScopes(),
  };
}

export const CUSTOM_THEMES_KEY = 'ui.themes';
export const KEYBINDINGS_KEY = 'ui.keybindings';

/** Applies custom themes and shortcut overrides stored in settings. */
async function loadUserCustomizations(
  transport: Transport,
  themes: ThemeManager,
  hotkeys: HotkeyRegistry,
) {
  try {
    const [stored, bindings] = await Promise.all([
      transport.call('settings.get', { key: CUSTOM_THEMES_KEY }),
      transport.call('settings.get', { key: KEYBINDINGS_KEY }),
    ]);
    for (const raw of Array.isArray(stored.value) ? stored.value : []) {
      const parsed = parseTheme(raw);
      if (parsed.ok) {
        try {
          themes.addCustom(parsed.theme);
        } catch (e) {
          console.warn('[themes] skipped custom theme', e);
        }
      }
    }
    const overrides = (bindings.value ?? {}) as Record<string, unknown>;
    for (const [id, combos] of Object.entries(overrides)) {
      if (Array.isArray(combos) && combos.every((c) => typeof c === 'string')) {
        try {
          hotkeys.setOverride(id, combos as string[]);
        } catch {
          // Invalid combo from an older version: ignore.
        }
      }
    }
  } catch (e) {
    console.warn('[settings] customizations not loaded', e);
  }
}

/** CSS font-family value: user font first, then the built-in stack. */
const fontStack = (user: string, fallback: string) =>
  user.trim() ? `${JSON.stringify(user.trim())}, ${fallback}` : fallback;

/** Keeps theme, locale and appearance in sync with preferences. Call inside a component. */
export function bindPreferences(services: Services, root: HTMLElement = document.documentElement) {
  $effect(() => services.themes.setPreference(services.prefs.value.theme));
  $effect(() => {
    const p = services.prefs.value;
    root.dataset.density = p.density;
    root.style.setProperty('--font-size', `${p.uiFontSize}px`);
    root.style.setProperty('--font-size-sm', `${p.uiFontSize - 1}px`);
    root.style.setProperty('--font-size-xs', `${p.uiFontSize - 2}px`);
    root.style.setProperty('--editor-font-size', `${p.editorFontSize}px`);
    root.style.setProperty(
      '--font-ui',
      fontStack(
        p.fontUi,
        "'Inter', system-ui, -apple-system, 'Segoe UI', Roboto, 'Noto Sans', sans-serif",
      ),
    );
    root.style.setProperty(
      '--font-mono',
      fontStack(
        p.fontMono,
        "'JetBrains Mono', 'Fira Code', ui-monospace, 'SF Mono', Menlo, Consolas, monospace",
      ),
    );
    root.style.setProperty('zoom', p.zoom === 100 ? '' : String(p.zoom / 100));
  });
  $effect(() => {
    const locale = services.prefs.value.locale;
    if (locale) i18n.setLocale(locale);
  });
}
