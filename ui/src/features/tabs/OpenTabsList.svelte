<!-- Open requests as a list (compact layout, instead of the tab strip). -->
<script lang="ts">
  import { t } from '$lib/i18n';
  import type { Services } from '$lib/services.svelte';
  import EmptyState from '$lib/ui/EmptyState.svelte';
  import Icon from '$lib/ui/Icon.svelte';
  import MethodBadge from '$lib/ui/MethodBadge.svelte';
  import Spinner from '$lib/ui/Spinner.svelte';
  import { closeTab, tabInfo } from './info.svelte';

  let { services, onselect }: { services: Services; onselect: () => void } = $props();
  const tabs = $derived(services.tabs);
</script>

{#if tabs.tabs.length === 0}
  <EmptyState title={t('tabs.none')} />
{:else}
  <ul class="list">
    {#each tabs.tabs as tab (tab.id)}
      {@const meta = tabInfo(services, tab)}
      <li class:active={tab.id === tabs.activeId}>
        <button
          type="button"
          class="open"
          aria-current={tab.id === tabs.activeId ? 'true' : undefined}
          onclick={() => {
            tabs.activate(tab.id);
            onselect();
          }}
        >
          <MethodBadge method={meta.method} protocol={meta.protocol} />
          <span class="name">{meta.name}</span>
          {#if meta.running}
            <Spinner size={12} />
          {:else if meta.dirty}
            <span class="dirty" aria-label={t('tabs.unsaved')}></span>
          {/if}
        </button>
        <button
          type="button"
          class="close"
          aria-label={t('tabs.close')}
          onclick={() => closeTab(services, tab.id)}
        >
          <Icon name="close" size={16} />
        </button>
      </li>
    {/each}
  </ul>
{/if}

<style>
  .list {
    list-style: none;
    margin: 0;
    padding: var(--space-1) 0;
    overflow: auto;
  }
  li {
    display: flex;
    align-items: center;
    border-bottom: 1px solid var(--c-border);
  }
  li.active {
    background: var(--c-bg-selected);
  }
  .open {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-height: var(--touch-target, 44px);
    padding: 0 var(--space-3);
    text-align: left;
  }
  .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .dirty {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--c-fg-muted);
  }
  .close {
    display: grid;
    place-items: center;
    width: var(--touch-target, 44px);
    height: var(--touch-target, 44px);
    color: var(--c-fg-muted);
  }
</style>
