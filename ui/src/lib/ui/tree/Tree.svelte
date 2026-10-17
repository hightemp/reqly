<!--
  Virtualized tree: keyboard navigation (WAI-ARIA tree pattern), multi-select,
  inline rename and pointer-based drag & drop (also between tree instances).
  Content of each row is provided by the `row` snippet.
-->
<script lang="ts" generics="T extends TreeItem">
  import { onMount, type Snippet } from 'svelte';
  import { portal } from '../overlay/portal';
  import VirtualList from '../VirtualList.svelte';
  import { treeDnd, type DropTarget } from './dnd.svelte';
  import {
    canDrop,
    dropPosition,
    flatten,
    normalizeSelection,
    rangeSelect,
    typeaheadIndex,
    type TreeItem,
    type TreeRow,
  } from './model';

  interface Props {
    items: T[];
    label: string;
    expanded?: string[];
    selected?: string[];
    /** Keyboard focus (roving). */
    active?: string | null;
    renaming?: string | null;
    filter?: (item: T) => boolean;
    rowHeight?: number;
    height?: number;
    draggable?: boolean;
    /** Accept drops (default: same as draggable). */
    droppable?: boolean;
    /** Restrict what may be dropped where (in addition to structural rules). */
    acceptDrop?: (ids: string[], target: DropTarget) => boolean;
    row: Snippet<[TreeRow<T>]>;
    empty?: Snippet;
    /** Single click on a row. */
    onactivate?: (id: string) => void;
    /** Enter or double click. */
    onopen?: (id: string) => void;
    onmove?: (ids: string[], target: DropTarget) => void;
    onrename?: (id: string, name: string) => void;
    oncontextmenu?: (e: MouseEvent, ids: string[]) => void;
    /** Keys the tree does not handle (for hotkeys like F2/Delete). */
    onkey?: (e: KeyboardEvent) => void;
    onfocuschange?: (focused: boolean) => void;
  }

  let {
    items,
    label,
    expanded = $bindable([]),
    selected = $bindable([]),
    active = $bindable(null),
    renaming = $bindable(null),
    filter,
    rowHeight = 26,
    height,
    draggable = true,
    droppable = draggable,
    acceptDrop,
    row: rowSnippet,
    empty,
    onactivate,
    onopen,
    onmove,
    onrename,
    oncontextmenu,
    onkey,
    onfocuschange,
  }: Props = $props();

  const treeId = `tree-${Math.random().toString(36).slice(2)}`;
  const expandedSet = $derived(new Set(expanded));
  const rows = $derived(flatten(items, expandedSet, filter));
  const visibleIds = $derived(rows.map((r) => r.id));
  const selectedSet = $derived(new Set(selected));
  let anchorId: string | null = null;
  let list: { scrollToIndex(i: number): void } | undefined = $state();
  let viewport: HTMLDivElement | null = $state(null);
  let renameValue = $state('');
  let typeBuffer = '';
  let typeTimer: ReturnType<typeof setTimeout> | undefined;
  let pending: { id: string; x: number; y: number } | null = null;
  let autoExpandTimer: ReturnType<typeof setTimeout> | undefined;

  const indexOf = (id: string | null) => (id === null ? -1 : visibleIds.indexOf(id));

  function setExpanded(id: string, open: boolean) {
    if (open && !expandedSet.has(id)) expanded = [...expanded, id];
    else if (!open && expandedSet.has(id)) expanded = expanded.filter((e) => e !== id);
  }

  function focusRow(id: string, select: 'single' | 'extend' | 'none' = 'single') {
    active = id;
    if (select === 'single') {
      selected = [id];
      anchorId = id;
    } else if (select === 'extend') {
      selected = rangeSelect(visibleIds, anchorId ?? id, id);
    }
    list?.scrollToIndex(indexOf(id));
    queueMicrotask(() =>
      viewport?.querySelector<HTMLElement>(`[data-row-id="${CSS.escape(id)}"]`)?.focus(),
    );
  }

  function onRowClick(e: MouseEvent, r: TreeRow<T>) {
    if (e.shiftKey) {
      selected = rangeSelect(visibleIds, anchorId ?? r.id, r.id);
      active = r.id;
    } else if (e.ctrlKey || e.metaKey) {
      selected = selectedSet.has(r.id) ? selected.filter((s) => s !== r.id) : [...selected, r.id];
      active = r.id;
      anchorId = r.id;
    } else {
      focusRow(r.id);
      onactivate?.(r.id);
    }
  }

  function onRowDblClick(r: TreeRow<T>) {
    if (r.item.isFolder) setExpanded(r.id, !r.expanded);
    onopen?.(r.id);
  }

  function onRowContextMenu(e: MouseEvent, r: TreeRow<T>) {
    if (!selectedSet.has(r.id)) focusRow(r.id);
    active = r.id;
    oncontextmenu?.(e, selectedSet.has(r.id) ? selected : [r.id]);
  }

  function onKeyDown(e: KeyboardEvent) {
    if (renaming) return;
    const i = indexOf(active);
    const current = rows[i];
    const extend = e.shiftKey ? 'extend' : 'single';
    switch (e.key) {
      case 'ArrowDown':
        if (rows.length) focusRow(rows[Math.min(i + 1, rows.length - 1)]!.id, extend);
        break;
      case 'ArrowUp':
        if (rows.length) focusRow(rows[Math.max(i - 1, 0)]!.id, extend);
        break;
      case 'Home':
        if (rows.length) focusRow(rows[0]!.id, extend);
        break;
      case 'End':
        if (rows.length) focusRow(rows[rows.length - 1]!.id, extend);
        break;
      case 'ArrowRight':
        if (!current?.item.isFolder) return;
        if (!current.expanded) setExpanded(current.id, true);
        else if (rows[i + 1] && rows[i + 1]!.depth > current.depth) focusRow(rows[i + 1]!.id);
        break;
      case 'ArrowLeft':
        if (!current) return;
        if (current.item.isFolder && current.expanded) setExpanded(current.id, false);
        else if (current.item.parentId && visibleIds.includes(current.item.parentId))
          focusRow(current.item.parentId);
        break;
      case 'Enter':
        if (!current) return;
        if (current.item.isFolder) setExpanded(current.id, !current.expanded);
        onopen?.(current.id);
        break;
      case ' ':
        if (!current) return;
        selected = selectedSet.has(current.id)
          ? selected.filter((s) => s !== current.id)
          : [...selected, current.id];
        break;
      case 'a':
        if (!(e.ctrlKey || e.metaKey)) return typeahead(e);
        selected = [...visibleIds];
        break;
      case 'Escape':
        if (!treeDnd.dragging) return onkey?.(e);
        treeDnd.end(false);
        break;
      default:
        if (e.key.length === 1 && !e.ctrlKey && !e.metaKey && !e.altKey) return typeahead(e);
        onkey?.(e);
        return;
    }
    e.preventDefault();
    e.stopPropagation();
  }

  function typeahead(e: KeyboardEvent) {
    clearTimeout(typeTimer);
    typeBuffer += e.key;
    typeTimer = setTimeout(() => (typeBuffer = ''), 700);
    const labels = rows.map((r) => r.item.label);
    const from = typeBuffer.length > 1 ? Math.max(indexOf(active) - 1, -1) : indexOf(active);
    const next = typeaheadIndex(labels, from < 0 ? labels.length - 1 : from, typeBuffer);
    if (next >= 0) focusRow(rows[next]!.id);
    e.preventDefault();
  }

  // ----- rename ---------------------------------------------------------------

  $effect(() => {
    if (renaming) {
      const item = items.find((i) => i.id === renaming);
      renameValue = item?.label ?? '';
    }
  });

  function commitRename() {
    const id = renaming;
    if (!id) return;
    renaming = null;
    const name = renameValue.trim();
    const item = items.find((i) => i.id === id);
    if (name && item && name !== item.label) onrename?.(id, name);
    focusRow(id, 'none');
  }

  function renameInput(node: HTMLInputElement) {
    node.focus();
    node.select();
  }

  // ----- drag & drop -------------------------------------------------------------

  function resolve(x: number, y: number, ids: string[]): DropTarget | null {
    if (!droppable || !viewport) return null;
    const rect = viewport.getBoundingClientRect();
    const offset = y - rect.top + viewport.scrollTop;
    const index = Math.floor(offset / rowHeight);
    const r = rows[index];
    let target: DropTarget;
    if (!r) {
      target = { treeId, id: null, position: 'inside' };
    } else {
      const ratio = (offset - index * rowHeight) / rowHeight;
      const position = dropPosition(ratio, r.item.isFolder);
      if (!canDrop(items, ids, r.id, position)) return null;
      target = { treeId, id: r.id, position };
      if (position === 'inside' && !r.expanded) scheduleAutoExpand(r.id);
    }
    if (acceptDrop && !acceptDrop(ids, target)) return null;
    return target;
  }

  function scheduleAutoExpand(id: string) {
    clearTimeout(autoExpandTimer);
    autoExpandTimer = setTimeout(() => {
      if (treeDnd.target?.id === id && treeDnd.target.position === 'inside') setExpanded(id, true);
    }, 600);
  }

  function autoScroll(y: number) {
    if (!viewport) return;
    const rect = viewport.getBoundingClientRect();
    if (y < rect.top + 24) viewport.scrollTop -= 8;
    else if (y > rect.bottom - 24) viewport.scrollTop += 8;
  }

  function onPointerDown(e: PointerEvent, r: TreeRow<T>) {
    if (!draggable || e.button !== 0 || renaming) return;
    pending = { id: r.id, x: e.clientX, y: e.clientY };
  }

  function onWindowPointerMove(e: PointerEvent) {
    if (pending && !treeDnd.dragging) {
      if (Math.hypot(e.clientX - pending.x, e.clientY - pending.y) < 5) return;
      const ids = selectedSet.has(pending.id) ? normalizeSelection(items, selected) : [pending.id];
      const first = items.find((i) => i.id === ids[0]);
      treeDnd.start(
        treeId,
        ids,
        ids.length > 1 ? `${first?.label ?? ''} +${ids.length - 1}` : (first?.label ?? ''),
      );
    }
    if (treeDnd.dragging?.source === treeId) {
      treeDnd.move(e.clientX, e.clientY);
      autoScroll(e.clientY);
    }
  }

  function onWindowPointerUp() {
    pending = null;
    if (treeDnd.dragging?.source === treeId) treeDnd.end(true);
  }

  onMount(() => {
    const unregister = treeDnd.register(treeId, {
      resolve,
      drop: (ids, target) => onmove?.(ids, target),
    });
    window.addEventListener('pointermove', onWindowPointerMove);
    window.addEventListener('pointerup', onWindowPointerUp);
    return () => {
      unregister();
      clearTimeout(autoExpandTimer);
      window.removeEventListener('pointermove', onWindowPointerMove);
      window.removeEventListener('pointerup', onWindowPointerUp);
    };
  });

  const dropHere = $derived(treeDnd.target?.treeId === treeId ? treeDnd.target : null);
  const dropIndex = $derived(dropHere?.id ? indexOf(dropHere.id) : -1);
</script>

<div
  class="tree"
  role="tree"
  aria-label={label}
  aria-multiselectable="true"
  data-tree-id={treeId}
  tabindex={rows.length === 0 ? 0 : -1}
  onkeydown={onKeyDown}
  onfocusin={() => onfocuschange?.(true)}
  onfocusout={(e) => {
    if (!(e.currentTarget as HTMLElement).contains(e.relatedTarget as Node)) onfocuschange?.(false);
  }}
  class:drop-root={dropHere?.id === null}
>
  {#if rows.length === 0}
    {@render empty?.()}
  {:else}
    <VirtualList bind:this={list} bind:viewport items={rows} {rowHeight} {height} key={(r) => r.id}>
      {#snippet row(r)}
        <!-- Keyboard interaction is handled by the tree container (roving tabindex). -->
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <div
          class="row"
          role="treeitem"
          data-row-id={r.id}
          tabindex={r.id === (active ?? rows[0]?.id) ? 0 : -1}
          aria-level={r.depth + 1}
          aria-selected={selectedSet.has(r.id)}
          aria-expanded={r.item.isFolder ? r.expanded : undefined}
          class:selected={selectedSet.has(r.id)}
          class:dragging={treeDnd.dragging?.ids.includes(r.id)}
          class:drop-inside={dropHere?.id === r.id && dropHere.position === 'inside'}
          style:height="{rowHeight}px"
          style:padding-left="{8 + r.depth * 14}px"
          onclick={(e) => onRowClick(e, r)}
          ondblclick={() => onRowDblClick(r)}
          oncontextmenu={(e) => onRowContextMenu(e, r)}
          onpointerdown={(e) => onPointerDown(e, r)}
        >
          {#if r.item.isFolder}
            <button
              type="button"
              class="chevron"
              class:open={r.expanded}
              tabindex="-1"
              aria-hidden="true"
              onclick={(e) => {
                e.stopPropagation();
                setExpanded(r.id, !r.expanded);
              }}
            >
              <svg viewBox="0 0 16 16" width="12" height="12"
                ><path
                  d="M6 4l4 4-4 4"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="1.6"
                  stroke-linecap="round"
                  stroke-linejoin="round"
                /></svg
              >
            </button>
          {:else}
            <span class="chevron-space"></span>
          {/if}
          {#if renaming === r.id}
            <input
              class="rename"
              aria-label={label}
              bind:value={renameValue}
              use:renameInput
              onclick={(e) => e.stopPropagation()}
              onpointerdown={(e) => e.stopPropagation()}
              onblur={commitRename}
              onkeydown={(e) => {
                e.stopPropagation();
                if (e.key === 'Enter') commitRename();
                if (e.key === 'Escape') {
                  renaming = null;
                  focusRow(r.id, 'none');
                }
              }}
            />
          {:else}
            {@render rowSnippet(r)}
          {/if}
        </div>
      {/snippet}
      {#snippet after()}
        {#if dropHere && dropHere.position !== 'inside' && dropIndex >= 0}
          {@const depth = rows[dropIndex]!.depth}
          <div
            class="drop-line"
            style:top="{(dropHere.position === 'before' ? dropIndex : dropIndex + 1) * rowHeight -
              1}px"
            style:left="{8 + depth * 14}px"
          ></div>
        {/if}
      {/snippet}
    </VirtualList>
  {/if}
</div>

{#if treeDnd.dragging?.source === treeId}
  <div
    use:portal
    class="ghost"
    style:left="{treeDnd.pointer.x + 12}px"
    style:top="{treeDnd.pointer.y + 8}px"
  >
    {treeDnd.dragging.label}
  </div>
{/if}

<style>
  .tree {
    height: 100%;
    outline: none;
    position: relative;
  }
  .tree.drop-root {
    box-shadow: inset 0 0 0 2px var(--c-accent);
  }
  .row {
    display: flex;
    align-items: center;
    gap: 4px;
    padding-right: var(--space-2);
    border-radius: var(--radius-sm);
    margin: 0 4px;
    cursor: default;
    outline: none;
    white-space: nowrap;
    overflow: hidden;
  }
  .row:hover {
    background: var(--c-bg-hover);
  }
  .row.selected {
    background: var(--c-bg-selected);
  }
  .row:focus-visible {
    box-shadow: inset 0 0 0 1px var(--c-focus-ring);
  }
  .row.dragging {
    opacity: 0.5;
  }
  .row.drop-inside {
    box-shadow: inset 0 0 0 2px var(--c-accent);
    background: var(--c-accent-subtle);
  }
  .chevron,
  .chevron-space {
    flex: none;
    width: 16px;
    height: 16px;
    display: grid;
    place-items: center;
    color: var(--c-fg-subtle);
  }
  .chevron svg {
    transition: transform var(--transition);
  }
  .chevron.open svg {
    transform: rotate(90deg);
  }
  .rename {
    flex: 1;
    min-width: 0;
    height: calc(100% - 4px);
    padding: 0 4px;
    border: 1px solid var(--c-accent);
    border-radius: var(--radius-sm);
    background: var(--c-bg-sunken);
    color: var(--c-fg);
    outline: none;
  }
  .drop-line {
    position: absolute;
    right: 8px;
    height: 2px;
    background: var(--c-accent);
    pointer-events: none;
    z-index: 2;
  }
  .ghost {
    position: fixed;
    z-index: 2000;
    pointer-events: none;
    padding: 2px 8px;
    border-radius: var(--radius-sm);
    background: var(--c-accent);
    color: var(--c-fg-on-accent);
    font-size: var(--font-size-sm);
    white-space: nowrap;
  }
</style>
