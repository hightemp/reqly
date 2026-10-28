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
  import CodegenModal from '../codegen/CodegenModal.svelte';
  import EnvironmentEditor from '../environments/EnvironmentEditor.svelte';
  import { interopDialogs } from '../interop/dialogs.svelte';
  import ExportDialog from '../interop/ExportDialog.svelte';
  import { runnableRequests } from '../runner/plan';
  import RunnerDialog from '../runner/RunnerDialog.svelte';
  import ImportWizard from '../interop/ImportWizard.svelte';
  import FolderSettings from '../folder/FolderSettings.svelte';
  import CollectionScripts from '../scripts/CollectionScripts.svelte';
  import CookieManager from '../modals/CookieManager.svelte';
  import MembersDialog from '../modals/MembersDialog.svelte';
  import RequestLog from '../modals/RequestLog.svelte';
  import TrashModal from '../modals/TrashModal.svelte';
  import CommandPalette, { type PaletteCommand } from '../palette/CommandPalette.svelte';
  import SettingsModal from '../settings/SettingsModal.svelte';
  import RequestPanel from '../request/RequestPanel.svelte';
  import ResponsePanel from '../response/ResponsePanel.svelte';
  import { createFolder, createRequest, REQUEST_KINDS, requestKindLabel } from '../sidebar/actions';
  import Sidebar from '../sidebar/Sidebar.svelte';
  import RequestTabs from '../tabs/RequestTabs.svelte';
  import { createTemplateHostFactory } from '../template/host';
  import PromptDialog from '../template/PromptDialog.svelte';
  import { promptState } from '../template/prompts.svelte';
  import TagEditor from '../template/TagEditor.svelte';
  import { tagEditor } from '../template/tagEditorState.svelte';
  import { provideTemplateHost } from '$lib/template/host';
  import { provideCollab } from '$lib/collab/collab';
  import Drawer from '$lib/ui/overlay/Drawer.svelte';
  import { installLongPress, swipeDirection, type TouchPoint } from '$lib/ui/touch';
  import CompactBar, { type CompactPane } from './CompactBar.svelte';
  import Titlebar from './Titlebar.svelte';

  interface Props {
    services: Services;
    /** Present when the app draws its own window frame. */
    windowController?: WindowController;
  }

  let { services, windowController }: Props = $props();
  // Services are created once at startup and never replaced.
  // svelte-ignore state_referenced_locally
  const { prefs, themes, hotkeys, transport, platform, workspace, tree, tabs, environments } =
    services;

  // svelte-ignore state_referenced_locally
  bindPreferences(services);
  provideTemplateHost(createTemplateHostFactory(environments));
  // svelte-ignore state_referenced_locally
  provideCollab(services.collab);
  // svelte-ignore state_referenced_locally
  services.responses.askPrompts = (prompts) => promptState.ask(prompts);
  // svelte-ignore state_referenced_locally
  services.streams.askPrompts = (prompts) => promptState.ask(prompts);

  // i18n-dynamic: languages.
  let offline = $state(false);
  let folderSettings = $state<string | null>(null);
  let settingsOpen = $state(false);
  let modal = $state<
    | 'palette'
    | 'cookies'
    | 'trash'
    | 'log'
    | 'environments'
    | 'collectionScripts'
    | 'members'
    | null
  >(null);

  // Compact (phone) layout: sidebar in a drawer, one pane at a time.
  let drawerOpen = $state(false);
  let pane = $state<CompactPane>('request');
  const compact = $derived(services.viewport.compact);

  // Opening a request closes the drawer and shows it.
  $effect(() => {
    void tabs.activeId;
    drawerOpen = false;
    pane = 'request';
  });
  /** Swipes start this close to the left edge to open the drawer. */
  const EDGE = 40;
  let swipeStart: TouchPoint | null = null;
  function onSwipeStart(e: PointerEvent) {
    swipeStart =
      compact && e.pointerType === 'touch' && e.isPrimary
        ? { x: e.clientX, y: e.clientY, t: e.timeStamp }
        : null;
  }
  function onSwipeEnd(e: PointerEvent) {
    if (!swipeStart || e.pointerType !== 'touch') return;
    const start = swipeStart;
    swipeStart = null;
    const direction = swipeDirection(start, { x: e.clientX, y: e.clientY, t: e.timeStamp });
    if (direction === 'right' && !drawerOpen && start.x <= EDGE) drawerOpen = true;
    else if (direction === 'left' && drawerOpen) drawerOpen = false;
  }

  // Sending shows the response.
  $effect(() => {
    const id = tabs.activeId;
    if (id && services.responses.run(id).sending) pane = 'response';
  });

  // Tell the others where this tab is (multi-user server).
  $effect(() => {
    services.presence.report(services.workspace.activeWorkspaceId, services.tabs.activeId);
  });

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
    const id = workspace.activeCollectionId;
    if (id && id !== environments.collectionId) void environments.load(id);
  });
  $effect(() => {
    // Tabs are loaded at startup; reload only when the workspace changes,
    // or a late load would overwrite tabs the user already changed.
    const id = workspace.activeWorkspaceId;
    if (id && id !== tabs.workspaceId) void tabs.load(id);
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
    ...REQUEST_KINDS.map((kind) => ({
      id: `new-${kind}`,
      label: requestKindLabel(kind),
      run: () => void createRequest(services, selectedOne(), kind),
    })),
    {
      id: 'new-folder',
      label: t('actions.newFolder'),
      run: () => void createFolder(services, selectedOne()),
    },
    { id: 'settings', label: t('settings.title'), run: () => (settingsOpen = true) },
    {
      id: 'environments',
      label: t('environments.manage'),
      run: () => (modal = 'environments'),
    },
    { id: 'import', label: t('import.menu'), run: () => interopDialogs.openImport() },
    ...(workspace.activeCollection
      ? [
          {
            id: 'export-collection',
            label: t('export.collection'),
            run: () =>
              interopDialogs.openExport({
                scope: {
                  kind: 'collection' as const,
                  collectionId: workspace.activeCollection!.id,
                },
                title: workspace.activeCollection!.name,
              }),
          },
        ]
      : []),
    ...(workspace.activeCollection
      ? [
          {
            id: 'run-collection',
            label: t('runner.collectionMenu'),
            run: () =>
              services.runner.open({
                collectionId: workspace.activeCollection!.id,
                folderId: null,
                name: workspace.activeCollection!.name,
              }),
          },
        ]
      : []),
    ...(tabs.activeId
      ? [
          {
            id: 'codegen',
            label: t('actions.generateCode'),
            run: () => interopDialogs.openCodegen(tabs.activeId!),
          },
        ]
      : []),
    ...environments.subs.map((e) => ({
      id: `env-${e.id}`,
      label: `${t('environments.label')}: ${e.name}`,
      run: () => void environments.setActive(e.id),
    })),
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
      hotkeys.on('environments.open', () => (modal = 'environments')),
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
    // Touch: holding a finger opens context menus; swipes move the drawer.
    offs.push(installLongPress(window));
    window.addEventListener('pointerdown', onSwipeStart, true);
    window.addEventListener('pointerup', onSwipeEnd, true);
    offs.push(() => {
      window.removeEventListener('pointerdown', onSwipeStart, true);
      window.removeEventListener('pointerup', onSwipeEnd, true);
    });
    const onBeforeUnload = () => {
      void services.editor.flushAll();
      void services.tabs.flush();
    };
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

  {#snippet sidebar(onCollapse: () => void)}
    <Sidebar
      {services}
      onFolderSettings={(id) => (folderSettings = id)}
      {onCollapse}
      onOpen={(m) => (modal = m)}
      onSettings={() => (settingsOpen = true)}
    />
  {/snippet}

  {#if compact}
    <main class="body compact">
      <CompactBar {services} bind:pane onmenu={() => (drawerOpen = true)} />
      {#if !services.account.canEdit(services.workspace.activeWorkspaceId)}
        <div class="read-only" role="status">{t('members.readOnly')}</div>
      {/if}
      <section class="pane">
        {#if pane === 'request'}
          <RequestPanel {services} />
        {:else}
          <ResponsePanel {services} />
        {/if}
      </section>
    </main>
    {#if drawerOpen}
      <Drawer
        label={t('sidebar.title')}
        closeLabel={t('shell.closeMenu')}
        onclose={() => (drawerOpen = false)}
      >
        {@render sidebar(() => (drawerOpen = false))}
      </Drawer>
    {/if}
  {:else}
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
          {@render sidebar(toggleSidebar)}
        {/snippet}
        {#snippet second()}
          <div class="workspace">
            {#if !services.account.canEdit(services.workspace.activeWorkspaceId)}
              <div class="read-only" role="status">{t('members.readOnly')}</div>
            {/if}
            <RequestTabs {services} />
            <div class="editor">
              <SplitPane
                direction={prefs.value.responseLayout === 'right' ? 'horizontal' : 'vertical'}
                bind:size={
                  () => prefs.value.requestPaneSize,
                  (v) => void prefs.update({ requestPaneSize: v })
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
  {/if}

  <OverlayHost />
  {#if modal === 'palette'}
    <CommandPalette {services} commands={paletteCommands} onclose={() => (modal = null)} />
  {:else if modal === 'cookies'}
    <CookieManager {services} onclose={() => (modal = null)} />
  {:else if modal === 'trash'}
    <TrashModal {services} onclose={() => (modal = null)} />
  {:else if modal === 'log'}
    <RequestLog {services} onclose={() => (modal = null)} />
  {:else if modal === 'environments'}
    <EnvironmentEditor {services} onclose={() => (modal = null)} />
  {:else if modal === 'members'}
    <MembersDialog {services} onclose={() => (modal = null)} />
  {:else if modal === 'collectionScripts' && services.workspace.activeCollection}
    <CollectionScripts
      {services}
      collection={services.workspace.activeCollection}
      onclose={() => (modal = null)}
    />
  {/if}
  {#if interopDialogs.import}
    <ImportWizard
      {services}
      collectionId={interopDialogs.import.collectionId}
      onclose={() => (interopDialogs.import = null)}
    />
  {/if}
  {#if interopDialogs.export}
    <ExportDialog
      {services}
      request={interopDialogs.export}
      onclose={() => (interopDialogs.export = null)}
    />
  {/if}
  {#if interopDialogs.codegen}
    <CodegenModal
      {services}
      requestId={interopDialogs.codegen}
      onclose={() => (interopDialogs.codegen = null)}
    />
  {/if}
  {#if services.runner.target}
    <RunnerDialog
      runner={services.runner}
      requests={runnableRequests(services.tree.items, services.runner.target.folderId)}
      environments={services.environments.subs}
      activeEnvironment={services.environments.active}
      onbeforerun={() => services.editor.flushAll()}
    />
  {/if}
  {#if tagEditor.request}
    <TagEditor {services} request={tagEditor.request} onclose={() => tagEditor.close()} />
  {/if}
  {#if promptState.request}
    <PromptDialog request={promptState.request} />
  {/if}
  {#if settingsOpen}
    <SettingsModal {services} onclose={() => (settingsOpen = false)} />
  {/if}
  {#if folderSettings}
    <FolderSettings {services} folderId={folderSettings} onclose={() => (folderSettings = null)} />
  {/if}

  <footer class="statusbar">
    {#if prefs.value.sidebarCollapsed && !compact}
      <IconButton icon="sidebar" size={14} label={t('sidebar.expand')} onclick={toggleSidebar} />
    {/if}
    <span class="spacer"></span>
    <IconButton
      icon="settings"
      size={14}
      label={t('settings.title')}
      onclick={() => (settingsOpen = true)}
    />
    {#if !compact}
      <IconButton
        icon={prefs.value.responseLayout === 'right' ? 'layout-right' : 'layout-bottom'}
        size={14}
        label={t('settings.layout')}
        onclick={toggleLayout}
      />
    {/if}
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
  .body.compact {
    display: flex;
    flex-direction: column;
  }
  .body.compact .pane {
    flex: 1;
    min-height: 0;
  }
  .workspace {
    height: 100%;
    display: flex;
    flex-direction: column;
  }
  .read-only {
    padding: var(--space-1) var(--space-3);
    border-bottom: 1px solid var(--c-border);
    background: var(--c-bg-sunken);
    color: var(--c-fg-subtle);
    font-size: var(--font-size-sm);
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
  @media (pointer: coarse) {
    .statusbar {
      --control-height: 36px;
    }
    .status-button {
      height: 36px;
    }
  }
</style>
