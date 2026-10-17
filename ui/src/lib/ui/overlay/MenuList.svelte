<!-- One level of a menu; submenus render recursively in their own popover. -->
<script lang="ts">
  import { onMount } from 'svelte';
  import MenuList from './MenuList.svelte';
  import Popover from './Popover.svelte';
  import { isItem, type MenuEntry, type MenuItem } from './menus.svelte';
  import type { Placement, Rect } from './position';

  interface Props {
    items: MenuEntry[];
    anchor: HTMLElement | Rect;
    placement?: Placement;
    /** Closes the whole menu tree. */
    onclose: (restoreFocus?: boolean) => void;
    /** Closes only this submenu (ArrowLeft); absent for the root. */
    onback?: () => void;
    autofocus?: boolean;
  }

  let {
    items,
    anchor,
    placement = 'bottom-start',
    onclose,
    onback,
    autofocus = true,
  }: Props = $props();

  let element: HTMLDivElement | null = $state(null);
  let openSub = $state<{ item: MenuItem; el: HTMLElement } | null>(null);

  const buttons = () => [
    ...(element?.querySelectorAll<HTMLElement>(
      ':scope > [role="menuitem"]:not([aria-disabled="true"])',
    ) ?? []),
  ];

  function focusIndex(i: number) {
    const all = buttons();
    if (all.length === 0) return;
    all[(i + all.length) % all.length]!.focus();
  }

  function currentIndex() {
    return buttons().indexOf(document.activeElement as HTMLElement);
  }

  function closeSub() {
    const el = openSub?.el;
    openSub = null;
    el?.focus();
  }

  /** Items in display order, aligned with `buttons()` indices of enabled entries. */
  function itemFor(el: Element | null): MenuItem | undefined {
    const index = Number((el as HTMLElement | null)?.dataset?.index);
    const entry = Number.isNaN(index) ? undefined : items[index];
    return entry && isItem(entry) ? entry : undefined;
  }

  function select(item: MenuItem, el: HTMLElement) {
    if (item.disabled) return;
    if (item.children) {
      openSub = { item, el };
      return;
    }
    onclose(false);
    item.onSelect?.();
  }

  function onKeyDown(e: KeyboardEvent) {
    const i = currentIndex();
    switch (e.key) {
      case 'ArrowDown':
        focusIndex(i + 1);
        break;
      case 'ArrowUp':
        focusIndex(i < 0 ? -1 : i - 1);
        break;
      case 'Home':
        focusIndex(0);
        break;
      case 'End':
        focusIndex(-1);
        break;
      case 'ArrowLeft':
        if (!onback) return;
        onback();
        break;
      case 'ArrowRight':
      case 'Enter':
      case ' ': {
        const el = document.activeElement as HTMLElement | null;
        const item = itemFor(el);
        if (!item || !el || (e.key === 'ArrowRight' && !item.children)) return;
        select(item, el);
        break;
      }
      case 'Escape':
        if (onback) onback();
        else onclose();
        break;
      case 'Tab':
        onclose();
        break;
      default:
        if (e.key.length === 1 && /\S/.test(e.key)) {
          const all = buttons();
          const start = Math.max(i, -1) + 1;
          const order = [...all.slice(start), ...all.slice(0, start)];
          order
            .find((b) => b.textContent?.trim().toLowerCase().startsWith(e.key.toLowerCase()))
            ?.focus();
          break;
        }
        return;
    }
    e.preventDefault();
    e.stopPropagation();
  }

  onMount(() => {
    if (autofocus) queueMicrotask(() => focusIndex(0));
  });
</script>

<Popover
  bind:element
  {anchor}
  {placement}
  offset={placement.startsWith('right') ? -4 : 2}
  role="menu"
  class="menu"
  data-menu-layer=""
  ignoreSelector="[data-menu-layer]"
  onclose={(reason) => (reason === 'escape' && onback ? onback() : onclose(reason === 'escape'))}
  onkeydown={onKeyDown}
>
  {#each items as entry, i (i)}
    {#if isItem(entry)}
      <!-- Keyboard is handled by the menu container (roving focus). -->
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <div
        role="menuitem"
        tabindex="-1"
        class="item"
        class:danger={entry.danger}
        aria-disabled={entry.disabled || undefined}
        aria-haspopup={entry.children ? 'menu' : undefined}
        aria-expanded={entry.children ? openSub?.item === entry : undefined}
        data-index={i}
        onclick={(e) => select(entry, e.currentTarget)}
        onpointerenter={(e) => {
          if (!entry.disabled) (e.currentTarget as HTMLElement).focus();
          if (entry.children && !entry.disabled)
            openSub = { item: entry, el: e.currentTarget as HTMLElement };
          else if (openSub && openSub.item !== entry) openSub = null;
        }}
      >
        <span class="check" aria-hidden="true">{entry.checked ? '✓' : ''}</span>
        <span class="label">{entry.label}</span>
        {#if entry.shortcut}<span class="shortcut">{entry.shortcut}</span>{/if}
        {#if entry.children}<span class="arrow" aria-hidden="true">›</span>{/if}
      </div>
    {:else if 'separator' in entry}
      <div class="separator" role="separator"></div>
    {:else}
      <div class="heading" role="presentation">{entry.heading}</div>
    {/if}
  {/each}
</Popover>

{#if openSub}
  <MenuList
    items={openSub?.item.children ?? []}
    anchor={openSub?.el ?? anchor}
    placement="right-start"
    {onclose}
    onback={closeSub}
  />
{/if}

<style>
  :global(.menu) {
    padding: 4px;
    min-width: 180px;
  }
  .item {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    height: var(--row-height);
    padding: 0 var(--space-2) 0 var(--space-1);
    border-radius: var(--radius-sm);
    cursor: default;
    outline: none;
    white-space: nowrap;
  }
  .item:focus {
    background: var(--c-accent);
    color: var(--c-fg-on-accent);
  }
  .item[aria-disabled='true'] {
    opacity: 0.45;
  }
  .danger {
    color: var(--c-danger);
  }
  .danger:focus {
    background: var(--c-danger);
  }
  .check {
    width: 14px;
    text-align: center;
  }
  .label {
    flex: 1;
  }
  .shortcut {
    color: var(--c-fg-subtle);
    font-size: var(--font-size-xs);
  }
  .item:focus .shortcut {
    color: inherit;
    opacity: 0.8;
  }
  .separator {
    height: 1px;
    margin: 4px 0;
    background: var(--c-border);
  }
  .heading {
    padding: 6px var(--space-2) 2px;
    font-size: var(--font-size-xs);
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--c-fg-subtle);
  }
</style>
