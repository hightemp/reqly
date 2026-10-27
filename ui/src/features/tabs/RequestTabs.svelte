<!-- Open requests as tabs (Postman-style). -->
<script lang="ts">
  import Avatars from '$lib/ui/Avatars.svelte';
  import { onMount } from 'svelte';
  import { t } from '$lib/i18n';
  import type { Services } from '$lib/services.svelte';
  import type { Tab } from '$lib/stores/tabs.svelte';
  import Icon from '$lib/ui/Icon.svelte';
  import IconButton from '$lib/ui/IconButton.svelte';
  import MethodBadge from '$lib/ui/MethodBadge.svelte';
  import { contextMenu, menus, type MenuEntry } from '$lib/ui/overlay/menus.svelte';
  import Spinner from '$lib/ui/Spinner.svelte';
  import { closeTab, tabInfo, trackTabs } from './info.svelte';

  let { services }: { services: Services } = $props();
  const tabs = $derived(services.tabs);
  let strip: HTMLDivElement | null = $state(null);
  let dragFrom = $state<number | null>(null);
  let dragOver = $state<number | null>(null);

  // svelte-ignore state_referenced_locally
  trackTabs(services);

  const info = (tab: Tab) => tabInfo(services, tab);
  const close = (id: string) => closeTab(services, id);

  function menuFor(tab: Tab, index: number): MenuEntry[] {
    return [
      { id: 'close', label: t('tabs.close'), onSelect: () => close(tab.id) },
      {
        id: 'others',
        label: t('tabs.closeOthers'),
        disabled: tabs.tabs.length < 2,
        onSelect: () => tabs.closeOthers(tab.id),
      },
      {
        id: 'right',
        label: t('tabs.closeRight'),
        disabled: index === tabs.tabs.length - 1,
        onSelect: () => tabs.closeRight(tab.id),
      },
      { id: 'all', label: t('tabs.closeAll'), onSelect: () => tabs.closeAll() },
      { separator: true },
      {
        id: 'pin',
        label: tab.pinned ? t('tabs.unpin') : t('tabs.pin'),
        onSelect: () => tabs.togglePin(tab.id),
      },
      {
        id: 'duplicate',
        label: t('actions.duplicate'),
        onSelect: async () => {
          await services.tree.duplicate([tab.id]);
          const copy = services.tree.selected[0];
          if (copy && copy !== tab.id) tabs.open(copy);
        },
      },
    ];
  }

  function overflowMenu(anchor: HTMLElement) {
    menus.openBelow(
      anchor,
      tabs.tabs.map((tab) => ({
        id: tab.id,
        label: info(tab).name,
        checked: tab.id === tabs.activeId,
        onSelect: () => tabs.activate(tab.id),
      })),
    );
  }

  function startDrag(e: PointerEvent, index: number) {
    if (e.button !== 0) return;
    const startX = e.clientX;
    const move = (ev: PointerEvent) => {
      if (dragFrom === null && Math.abs(ev.clientX - startX) > 5) dragFrom = index;
      if (dragFrom === null) return;
      const el = document
        .elementFromPoint(ev.clientX, ev.clientY)
        ?.closest<HTMLElement>('[data-tab-index]');
      if (el) dragOver = Number(el.dataset.tabIndex);
    };
    const up = () => {
      if (dragFrom !== null && dragOver !== null && dragFrom !== dragOver)
        tabs.move(dragFrom, dragOver);
      dragFrom = null;
      dragOver = null;
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', up);
    };
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', up);
  }

  $effect(() => {
    const id = tabs.activeId;
    if (id)
      queueMicrotask(() =>
        strip
          ?.querySelector(`[data-tab-id="${CSS.escape(id)}"]`)
          ?.scrollIntoView({ block: 'nearest', inline: 'nearest' }),
      );
  });

  onMount(() => {
    const h = services.hotkeys;
    const offs = [
      h.on('tab.close', () => tabs.activeId && close(tabs.activeId)),
      h.on('tab.reopen', () =>
        tabs.reopenClosed((id) => !services.editor.get(id)?.node?.deletedAt),
      ),
      h.on('tab.next', () => tabs.next()),
      h.on('tab.prev', () => tabs.prev()),
      ...[1, 2, 3, 4, 5, 6, 7, 8, 9].map((n) =>
        h.on(`tab.goto${n}`, () => tabs.activateIndex(n - 1)),
      ),
    ];
    return () => offs.forEach((off) => off());
  });
</script>

{#if tabs.tabs.length > 0}
  <div class="bar">
    <div bind:this={strip} class="strip" role="tablist" aria-label={t('tabs.label')}>
      {#each tabs.tabs as tab, i (tab.id)}
        {@const meta = info(tab)}
        <div
          class="tab"
          role="tab"
          tabindex={tab.id === tabs.activeId ? 0 : -1}
          aria-selected={tab.id === tabs.activeId}
          data-tab-id={tab.id}
          data-tab-index={i}
          class:preview={tab.preview}
          class:drop={dragOver === i && dragFrom !== i}
          onclick={() => tabs.activate(tab.id)}
          ondblclick={() => tabs.makePermanent(tab.id)}
          onauxclick={(e) => e.button === 1 && close(tab.id)}
          onkeydown={(e) => {
            if (e.key === 'Enter' || e.key === ' ') tabs.activate(tab.id);
          }}
          oncontextmenu={(e) => contextMenu(e, menuFor(tab, i))}
          onpointerdown={(e) => startDrag(e, i)}
        >
          {#if tab.pinned}<span class="pin"><Icon name="pin" size={11} /></span>{/if}
          <MethodBadge method={meta.method} protocol={meta.protocol} />
          <span class="name">{meta.name}</span>
          <Avatars
            people={services.presence.on(tab.id)}
            size={14}
            max={2}
            label={t('presence.open')}
          />
          <span class="state">
            {#if meta.running}
              <Spinner size={10} />
            {:else if meta.dirty}
              <span class="dirty" aria-label={t('tabs.unsaved')}></span>
            {/if}
          </span>
          {#if !tab.pinned}
            <button
              type="button"
              class="close"
              tabindex="-1"
              aria-label={t('tabs.close')}
              onclick={(e) => {
                e.stopPropagation();
                close(tab.id);
              }}
              onpointerdown={(e) => e.stopPropagation()}
            >
              <Icon name="close" size={11} />
            </button>
          {/if}
        </div>
      {/each}
    </div>
    <IconButton
      icon="more"
      size={14}
      label={t('tabs.all')}
      onclick={(e) => overflowMenu(e.currentTarget)}
    />
  </div>
{/if}

<style>
  .bar {
    display: flex;
    align-items: stretch;
    height: 36px;
    flex: none;
    border-bottom: 1px solid var(--c-border);
    background: var(--c-bg-elevated);
    padding-right: var(--space-1);
  }
  .bar :global(.icon-button) {
    align-self: center;
  }
  .strip {
    flex: 1;
    min-width: 0;
    display: flex;
    overflow-x: auto;
    scrollbar-width: none;
  }
  .strip::-webkit-scrollbar {
    display: none;
  }
  .tab {
    flex: none;
    display: flex;
    align-items: center;
    gap: 6px;
    max-width: 220px;
    padding: 0 6px 0 var(--space-2);
    border-right: 1px solid var(--c-border);
    color: var(--c-fg-muted);
    cursor: default;
    outline: none;
    position: relative;
  }
  .tab:hover {
    background: var(--c-bg-hover);
  }
  .tab[aria-selected='true'] {
    background: var(--c-bg);
    color: var(--c-fg);
    box-shadow: inset 0 -2px 0 var(--c-accent);
  }
  .tab:focus-visible {
    box-shadow: inset 0 0 0 1px var(--c-focus-ring);
  }
  .tab.preview .name {
    font-style: italic;
  }
  .tab.drop {
    box-shadow: inset 2px 0 0 var(--c-accent);
  }
  .tab :global(.method) {
    min-width: 0;
  }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .pin {
    color: var(--c-fg-subtle);
  }
  .state {
    width: 10px;
    display: grid;
    place-items: center;
  }
  .dirty {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--c-fg-muted);
  }
  .close {
    width: 18px;
    height: 18px;
    display: grid;
    place-items: center;
    border-radius: var(--radius-sm);
    color: var(--c-fg-subtle);
    opacity: 0;
  }
  .tab:hover .close,
  .tab[aria-selected='true'] .close {
    opacity: 1;
  }
  .close:hover {
    background: var(--c-bg-active);
    color: var(--c-fg);
  }
</style>
