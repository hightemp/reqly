// Application-wide services, created once at startup and passed to the shell.

import type { Transport } from './api/transport';
import { defaultActions } from './hotkeys/actions';
import { HotkeyRegistry, detectPlatform, type Platform } from './hotkeys/hotkeys';
import { i18n } from './i18n';
import { PreferencesStore } from './preferences.svelte';
import { EditorStore } from './stores/editor.svelte';
import { ResponseStore } from './stores/responses.svelte';
import { TabsStore } from './stores/tabs.svelte';
import { TreeStore } from './stores/tree.svelte';
import { WorkspaceStore } from './stores/workspace.svelte';
import { ThemeManager, type MediaQuery } from './themes/manager.svelte';

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

  const workspace = new WorkspaceStore(transport, prefs);
  const tree = new TreeStore(transport);
  const tabs = new TabsStore(transport);
  const editor = new EditorStore(transport, () => prefs.value.autosave);
  const responses = new ResponseStore(transport, (id) => editor.flush(id));
  try {
    await workspace.load();
    if (workspace.activeCollectionId) await tree.load(workspace.activeCollectionId);
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
    scopes: new FocusScopes(),
  };
}

/** Keeps theme and locale in sync with preferences. Call inside a component. */
export function bindPreferences(services: Services) {
  $effect(() => services.themes.setPreference(services.prefs.value.theme));
  $effect(() => {
    const locale = services.prefs.value.locale;
    if (locale) i18n.setLocale(locale);
  });
}
