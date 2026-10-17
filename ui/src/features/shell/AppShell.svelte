<!--
  Top-level layout: [titlebar] / sidebar | request editor | response / status bar.
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import { TRANSPORT_TOPICS } from '$lib/api/transport';
  import { LOCALES, i18n, t } from '$lib/i18n';
  import type { WindowController } from '$lib/platform/window';
  import { bindPreferences, type Services } from '$lib/services.svelte';
  import type { IconName } from '$lib/ui/Icon.svelte';
  import IconButton from '$lib/ui/IconButton.svelte';
  import SplitPane from '$lib/ui/SplitPane.svelte';
  import Titlebar from './Titlebar.svelte';

  interface Props {
    services: Services;
    /** Present when the app draws its own window frame. */
    windowController?: WindowController;
  }

  let { services, windowController }: Props = $props();
  // Services are created once at startup and never replaced.
  // svelte-ignore state_referenced_locally
  const { prefs, themes, hotkeys, transport, platform } = services;

  // svelte-ignore state_referenced_locally
  bindPreferences(services);

  // i18n-dynamic: languages.
  let offline = $state(false);
  const scopes = ['global'];

  const toggleSidebar = () => prefs.update({ sidebarCollapsed: !prefs.value.sidebarCollapsed });
  const toggleLayout = () =>
    prefs.update({ responseLayout: prefs.value.responseLayout === 'right' ? 'bottom' : 'right' });

  const THEME_CYCLE = ['system', 'light', 'dark'];
  const THEME_ICON: Record<string, IconName> = { system: 'monitor', light: 'sun', dark: 'moon' };
  const themeLabel = $derived(
    prefs.value.theme === 'system'
      ? t('settings.themeSystem')
      : (themes.themes.find((x) => x.id === prefs.value.theme)?.name ?? prefs.value.theme),
  );

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
      transport.subscribe((e) => {
        if (e.topic === TRANSPORT_TOPICS.disconnected) offline = true;
        if (e.topic === TRANSPORT_TOPICS.reconnected) {
          offline = false;
          void prefs.load();
        }
      }),
    ];
    const onKeyDown = (e: KeyboardEvent) => hotkeys.handle(e, scopes);
    window.addEventListener('keydown', onKeyDown);
    return () => {
      offs.forEach((off) => off());
      window.removeEventListener('keydown', onKeyDown);
    };
  });
</script>

<div class="app">
  {#if windowController}
    <Titlebar controller={windowController} {platform} />
  {/if}

  {#if offline}
    <div class="banner" role="status">{t('app.connectionLost')}</div>
  {/if}

  <main class="body">
    <SplitPane
      bind:size={() => prefs.value.sidebarWidth, (v) => void prefs.update({ sidebarWidth: v })}
      collapsed={prefs.value.sidebarCollapsed}
      min={180}
      max={600}
      defaultSize={280}
      label={t('sidebar.title')}
    >
      {#snippet first()}
        <aside class="sidebar" aria-label={t('sidebar.title')}>
          <header class="panel-header">
            <h2>{t('sidebar.title')}</h2>
            <IconButton icon="sidebar" label={t('sidebar.collapse')} onclick={toggleSidebar} />
          </header>
          <p class="empty">{t('sidebar.empty')}</p>
        </aside>
      {/snippet}
      {#snippet second()}
        <SplitPane
          direction={prefs.value.responseLayout === 'right' ? 'horizontal' : 'vertical'}
          bind:size={
            () => prefs.value.requestPaneSize, (v) => void prefs.update({ requestPaneSize: v })
          }
          min={240}
          max={1600}
          defaultSize={520}
          label={t('request.empty')}
        >
          {#snippet first()}
            <section class="panel"><p class="empty">{t('request.empty')}</p></section>
          {/snippet}
          {#snippet second()}
            <section class="panel response"><p class="empty">{t('response.empty')}</p></section>
          {/snippet}
        </SplitPane>
      {/snippet}
    </SplitPane>
  </main>

  <footer class="statusbar">
    {#if prefs.value.sidebarCollapsed}
      <IconButton icon="sidebar" size={14} label={t('sidebar.expand')} onclick={toggleSidebar} />
    {/if}
    <span class="spacer"></span>
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
  .sidebar {
    height: 100%;
    display: flex;
    flex-direction: column;
    background: var(--c-bg-elevated);
  }
  .panel-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    height: 40px;
    padding: 0 var(--space-2) 0 var(--space-3);
    border-bottom: 1px solid var(--c-border);
  }
  .panel-header h2 {
    font-weight: 600;
  }
  .panel {
    height: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .response {
    background: var(--c-bg-elevated);
  }
  .empty {
    color: var(--c-fg-subtle);
    padding: var(--space-4);
    text-align: center;
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
