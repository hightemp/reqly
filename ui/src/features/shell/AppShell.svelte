<!--
  Top-level layout: [titlebar] / sidebar | tabs + request editor | response / status bar.
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import { TRANSPORT_TOPICS } from '$lib/api/transport';
  import { LOCALES, i18n, t } from '$lib/i18n';
  import type { WindowController } from '$lib/platform/window';
  import { bindPreferences, type Services } from '$lib/services.svelte';
  import type { IconName } from '$lib/ui/Icon.svelte';
  import IconButton from '$lib/ui/IconButton.svelte';
  import OverlayHost from '$lib/ui/overlay/OverlayHost.svelte';
  import SplitPane from '$lib/ui/SplitPane.svelte';
  import FolderSettings from '../folder/FolderSettings.svelte';
  import CookieManager from '../modals/CookieManager.svelte';
  import RequestLog from '../modals/RequestLog.svelte';
  import TrashModal from '../modals/TrashModal.svelte';
  import CommandPalette, { type PaletteCommand } from '../palette/CommandPalette.svelte';
  import SettingsModal from '../settings/SettingsModal.svelte';
  import RequestPanel from '../request/RequestPanel.svelte';
  import ResponsePanel from '../response/ResponsePanel.svelte';
  import { createFolder, createRequest } from '../sidebar/actions';
  import Sidebar from '../sidebar/Sidebar.svelte';
  import RequestTabs from '../tabs/RequestTabs.svelte';
  import Titlebar from './Titlebar.svelte';

  interface Props {
    services: Services;
    /** Present when the app draws its own window frame. */
    windowController?: WindowController;
  }

  let { services, windowController }: Props = $props();
  // Services are created once at startup and never replaced.
  // svelte-ignore state_referenced_locally
  const { prefs, themes, hotkeys, transport, platform, workspace, tree, tabs } = services;

  // svelte-ignore state_referenced_locally
  bindPreferences(services);

  // i18n-dynamic: languages.
  let offline = $state(false);
  let folderSettings = $state<string | null>(null);
  let settingsOpen = $state(false);
  let modal = $state<'palette' | 'cookies' | 'trash' | 'log' | null>(null);

  // Desktop: custom title bar or the OS frame.
  $effect(() => {
    void windowController?.setDecorations(prefs.value.nativeFrame);
  });

  // Follow the active collection / workspace.
  $effect(() => {
    const id = workspace.activeCollectionId;
    if (id && id !== tree.collectionId) void tree.load(id);
  });
  $effect(() => {
    const id = workspace.activeWorkspaceId;
    if (id) void tabs.load(id);
  });

  const toggleSidebar = () => prefs.update({ sidebarCollapsed: !prefs.value.sidebarCollapsed });
  const toggleLayout = () =>
    prefs.update({ responseLayout: prefs.value.responseLayout === 'right' ? 'bottom' : 'right' });
  const selectedOne = () => (tree.selected.length === 1 ? tree.selected[0]! : null);

  const THEME_CYCLE = ['system', 'light', 'dark'];
  const THEME_ICON: Record<string, IconName> = { system: 'monitor', light: 'sun', dark: 'moon' };
  const themeLabel = $derived(
    prefs.value.theme === 'system'
      ? t('settings.themeSystem')
      : (themes.themes.find((x) => x.id === prefs.value.theme)?.name ?? prefs.value.theme),
  );

  const paletteCommands = $derived<PaletteCommand[]>([
    {
      id: 'new-request',
      label: t('actions.newRequest'),
      run: () => void createRequest(services, selectedOne()),
    },
    {
      id: 'new-folder',
      label: t('actions.newFolder'),
      run: () => void createFolder(services, selectedOne()),
    },
    { id: 'settings', label: t('settings.title'), run: () => (settingsOpen = true) },
    { id: 'cookies', label: t('cookieManager.title'), run: () => (modal = 'cookies') },
    { id: 'log', label: t('log.title'), run: () => (modal = 'log') },
    { id: 'trash', label: t('trash.title'), run: () => (modal = 'trash') },
    { id: 'sidebar', label: t('hotkeys.sidebar.toggle'), run: () => void toggleSidebar() },
    { id: 'layout', label: t('settings.layout'), run: () => void toggleLayout() },
    {
      id: 'sidebar-mode',
      label:
        prefs.value.sidebarMode === 'tree' ? t('sidebar.modeCategories') : t('sidebar.modeTree'),
      run: () =>
        void prefs.update({
          sidebarMode: prefs.value.sidebarMode === 'tree' ? 'categories' : 'tree',
        }),
    },
    ...themes.themes.map((th) => ({
      id: `theme-${th.id}`,
      label: `${t('palette.theme')}: ${th.name}`,
      run: () => void prefs.update({ theme: th.id }),
    })),
    ...workspace.collections.map((c) => ({
      id: `col-${c.id}`,
      label: `${t('palette.collection')}: ${c.name}`,
      run: () => void workspace.selectCollection(c.id),
    })),
    ...workspace.workspaces.map((w) => ({
      id: `ws-${w.id}`,
      label: `${t('palette.workspace')}: ${w.name}`,
      run: () => void workspace.selectWorkspace(w.id),
    })),
  ]);

  function cycleTheme() {
    const index = THEME_CYCLE.indexOf(prefs.value.theme);
    void prefs.update({ theme: THEME_CYCLE[(index + 1) % THEME_CYCLE.length]! });
  }

  function cycleLocale() {
    const index = LOCALES.indexOf(i18n.locale);
    const next = LOCALES[(index + 1) % LOCALES.length]!;
    i18n.setLocale(next);
    void prefs.update({ locale: next });
  }

  onMount(() => {
    const offs = [
      hotkeys.on('sidebar.toggle', toggleSidebar),
      hotkeys.on('layout.toggle', toggleLayout),
      hotkeys.on('request.new', () => void createRequest(services, selectedOne())),
      hotkeys.on('folder.new', () => void createFolder(services, selectedOne())),
      hotkeys.on('settings.open', () => (settingsOpen = true)),
      hotkeys.on('palette.open', () => (modal = 'palette')),
      transport.subscribe((e) => {
        if (e.topic === TRANSPORT_TOPICS.disconnected) offline = true;
        if (e.topic === TRANSPORT_TOPICS.reconnected) {
          offline = false;
          void prefs.load();
          void tree.reload();
        }
      }),
    ];
    const onKeyDown = (e: KeyboardEvent) => {
      // Let text fields keep their own shortcuts (Ctrl+A/C/V…) unless the
      // combo is a global app command.
      hotkeys.handle(e, services.scopes.list);
    };
    window.addEventListener('keydown', onKeyDown);
    const onBeforeUnload = () => void services.editor.flushAll();
    window.addEventListener('beforeunload', onBeforeUnload);
    return () => {
      offs.forEach((off) => off());
      window.removeEventListener('keydown', onKeyDown);
      window.removeEventListener('beforeunload', onBeforeUnload);
    };
  });
</script>

<div class="app">
  {#if windowController && !prefs.value.nativeFrame}
    <Titlebar controller={windowController} {platform} />
  {/if}

  {#if offline}
    <div class="banner" role="status">{t('app.connectionLost')}</div>
  {/if}

  <main class="body">
    <SplitPane
      bind:size={() => prefs.value.sidebarWidth, (v) => void prefs.update({ sidebarWidth: v })}
      collapsed={prefs.value.sidebarCollapsed}
      min={200}
      max={640}
      defaultSize={280}
      label={t('sidebar.title')}
    >
      {#snippet first()}
        <Sidebar
          {services}
          onFolderSettings={(id) => (folderSettings = id)}
          onCollapse={toggleSidebar}
          onOpen={(m) => (modal = m)}
        />
      {/snippet}
      {#snippet second()}
        <div class="workspace">
          <RequestTabs {services} />
          <div class="editor">
            <SplitPane
              direction={prefs.value.responseLayout === 'right' ? 'horizontal' : 'vertical'}
              bind:size={
                () => prefs.value.requestPaneSize, (v) => void prefs.update({ requestPaneSize: v })
              }
              min={260}
              max={1600}
              defaultSize={520}
              label={t('request.sections')}
            >
              {#snippet first()}
                <section class="pane"><RequestPanel {services} /></section>
              {/snippet}
              {#snippet second()}
                <section class="pane"><ResponsePanel {services} /></section>
              {/snippet}
            </SplitPane>
          </div>
        </div>
      {/snippet}
    </SplitPane>
  </main>

  <OverlayHost />
  {#if modal === 'palette'}
    <CommandPalette {services} commands={paletteCommands} onclose={() => (modal = null)} />
  {:else if modal === 'cookies'}
    <CookieManager {services} onclose={() => (modal = null)} />
  {:else if modal === 'trash'}
    <TrashModal {services} onclose={() => (modal = null)} />
  {:else if modal === 'log'}
    <RequestLog {services} onclose={() => (modal = null)} />
  {/if}
  {#if settingsOpen}
    <SettingsModal {services} onclose={() => (settingsOpen = false)} />
  {/if}
  {#if folderSettings}
    <FolderSettings {services} folderId={folderSettings} onclose={() => (folderSettings = null)} />
  {/if}

  <footer class="statusbar">
    {#if prefs.value.sidebarCollapsed}
      <IconButton icon="sidebar" size={14} label={t('sidebar.expand')} onclick={toggleSidebar} />
    {/if}
    <span class="spacer"></span>
    <IconButton
      icon="settings"
      size={14}
      label={t('settings.title')}
      onclick={() => (settingsOpen = true)}
    />
    <IconButton
      icon={prefs.value.responseLayout === 'right' ? 'layout-right' : 'layout-bottom'}
      size={14}
      label={t('settings.layout')}
      onclick={toggleLayout}
    />
    <IconButton
      icon={THEME_ICON[prefs.value.theme] ?? 'sun'}
      size={14}
      label="{t('settings.theme')}: {themeLabel}"
      onclick={cycleTheme}
    />
    <button
      type="button"
      class="status-button"
      aria-label="{t('settings.language')}: {t(`languages.${i18n.locale}`)}"
      onclick={cycleLocale}
    >
      {i18n.locale.toUpperCase()}
    </button>
  </footer>
</div>

<style>
  .app {
    display: flex;
    flex-direction: column;
    height: 100%;
  }
  .banner {
    padding: var(--space-1) var(--space-3);
    background: var(--c-warning-subtle);
    color: var(--c-warning);
    font-size: var(--font-size-sm);
  }
  .body {
    flex: 1;
    min-height: 0;
  }
  .workspace {
    height: 100%;
    display: flex;
    flex-direction: column;
  }
  .editor {
    flex: 1;
    min-height: 0;
  }
  .pane {
    height: 100%;
    min-width: 0;
  }
  .statusbar {
    display: flex;
    align-items: center;
    gap: 2px;
    height: var(--statusbar-height);
    padding: 0 var(--space-1);
    flex: none;
    border-top: 1px solid var(--c-border);
    background: var(--c-bg-elevated);
    --control-height: 20px;
  }
  .spacer {
    flex: 1;
  }
  .status-button {
    height: 20px;
    padding: 0 var(--space-2);
    border-radius: var(--radius-sm);
    font-size: var(--font-size-xs);
    font-weight: 600;
    color: var(--c-fg-muted);
  }
  .status-button:hover {
    background: var(--c-bg-hover);
    color: var(--c-fg);
  }
</style>
