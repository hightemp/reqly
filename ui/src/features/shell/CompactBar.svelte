<!--
  Top bar of the compact (phone) layout: menu (sidebar drawer), the open
  requests and the request / response switch.
-->
<script module lang="ts">
  export type CompactPane = 'request' | 'response';
</script>

<script lang="ts">
  import { t } from '$lib/i18n';
  import type { Services } from '$lib/services.svelte';
  import Icon from '$lib/ui/Icon.svelte';
  import IconButton from '$lib/ui/IconButton.svelte';
  import Drawer from '$lib/ui/overlay/Drawer.svelte';
  import { tabInfo, trackTabs } from '../tabs/info.svelte';
  import OpenTabsList from '../tabs/OpenTabsList.svelte';

  interface Props {
    services: Services;
    pane: CompactPane;
    onmenu: () => void;
  }

  let { services, pane = $bindable(), onmenu }: Props = $props();
  // i18n-dynamic: shell.request
  // i18n-dynamic: shell.response
  let listOpen = $state(false);
  // svelte-ignore state_referenced_locally
  trackTabs(services);

  const active = $derived(services.tabs.tabs.find((tab) => tab.id === services.tabs.activeId));
  const title = $derived(active ? tabInfo(services, active).name : t('tabs.label'));
  const count = $derived(services.tabs.tabs.length);
</script>

<header class="bar">
  <IconButton icon="menu" size={18} label={t('shell.menu')} onclick={onmenu} />
  <button
    type="button"
    class="tabs"
    aria-label={t('shell.openTabs', { count })}
    onclick={() => (listOpen = true)}
  >
    <span class="title">{title}</span>
    <span class="count">{count}</span>
    <Icon name="arrow-down" size={12} />
  </button>
  <div class="switch" role="group">
    {#each ['request', 'response'] as const as p (p)}
      <button type="button" aria-pressed={pane === p} onclick={() => (pane = p)}>
        {t(`shell.${p}`)}
      </button>
    {/each}
  </div>
</header>

{#if listOpen}
  <Drawer
    side="bottom"
    label={t('tabs.label')}
    closeLabel={t('common.close')}
    onclose={() => (listOpen = false)}
  >
    <OpenTabsList {services} onselect={() => (listOpen = false)} />
  </Drawer>
{/if}

<style>
  .bar {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    min-height: 44px;
    padding: 0 var(--space-1);
    flex: none;
    border-bottom: 1px solid var(--c-border);
    background: var(--c-bg-elevated);
    --control-height: 36px;
  }
  .tabs {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: var(--space-1);
    height: 36px;
    padding: 0 var(--space-2);
    border-radius: var(--radius-sm);
  }
  .tabs:hover {
    background: var(--c-bg-hover);
  }
  .title {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: 600;
  }
  .count {
    padding: 0 6px;
    border-radius: 999px;
    background: var(--c-bg-sunken);
    color: var(--c-fg-muted);
    font-size: var(--font-size-xs);
  }
  .switch {
    display: flex;
    flex: none;
    padding: 2px;
    border-radius: var(--radius);
    background: var(--c-bg-sunken);
  }
  .switch button {
    height: 32px;
    padding: 0 var(--space-2);
    border-radius: var(--radius-sm);
    color: var(--c-fg-muted);
    font-size: var(--font-size-sm);
  }
  .switch button[aria-pressed='true'] {
    background: var(--c-bg);
    color: var(--c-fg);
    box-shadow: 0 1px 2px var(--c-shadow);
  }
</style>
