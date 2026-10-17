<!-- Tab list (WAI-ARIA tabs, automatic activation). Panels are rendered by the caller. -->
<script lang="ts" module>
  export interface TabSpec<V extends string = string> {
    id: V;
    label: string;
    count?: number;
    /** Small dot indicator (e.g. "has content"). */
    dot?: boolean;
    disabled?: boolean;
  }
</script>

<script lang="ts" generics="K extends string">
  interface Props {
    tabs: TabSpec<K>[];
    value: K;
    label: string;
    idPrefix?: string;
    onchange?: (id: K) => void;
  }

  let { tabs, value = $bindable(), label, idPrefix = 'tabs', onchange }: Props = $props();
  let list: HTMLDivElement | null = $state(null);

  function select(id: K) {
    value = id;
    onchange?.(id);
  }

  function onKeyDown(e: KeyboardEvent) {
    const enabled = tabs.filter((t) => !t.disabled);
    const i = enabled.findIndex((t) => t.id === value);
    let next: TabSpec<K> | undefined;
    if (e.key === 'ArrowRight') next = enabled[(i + 1) % enabled.length];
    else if (e.key === 'ArrowLeft') next = enabled[(i - 1 + enabled.length) % enabled.length];
    else if (e.key === 'Home') next = enabled[0];
    else if (e.key === 'End') next = enabled[enabled.length - 1];
    if (!next) return;
    e.preventDefault();
    select(next.id);
    list?.querySelector<HTMLElement>(`#${idPrefix}-tab-${next.id}`)?.focus();
  }
</script>

<div
  bind:this={list}
  class="tabs"
  role="tablist"
  aria-label={label}
  tabindex="-1"
  onkeydown={onKeyDown}
>
  {#each tabs as tab (tab.id)}
    <button
      type="button"
      role="tab"
      id="{idPrefix}-tab-{tab.id}"
      aria-controls="{idPrefix}-panel-{tab.id}"
      aria-selected={tab.id === value}
      tabindex={tab.id === value ? 0 : -1}
      disabled={tab.disabled}
      class="tab"
      onclick={() => select(tab.id)}
    >
      {tab.label}
      {#if tab.count}<span class="count">{tab.count}</span>{/if}
      {#if tab.dot}<span class="dot" aria-hidden="true"></span>{/if}
    </button>
  {/each}
</div>

<style>
  .tabs {
    display: flex;
    align-items: stretch;
    gap: 2px;
    overflow-x: auto;
    scrollbar-width: none;
    outline: none;
  }
  .tabs::-webkit-scrollbar {
    display: none;
  }
  .tab {
    position: relative;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 0 var(--space-3);
    height: 34px;
    color: var(--c-fg-muted);
    white-space: nowrap;
    border-bottom: 2px solid transparent;
  }
  .tab:hover {
    color: var(--c-fg);
  }
  .tab[aria-selected='true'] {
    color: var(--c-fg);
    border-bottom-color: var(--c-accent);
  }
  .tab:disabled {
    opacity: 0.4;
  }
  .count {
    min-width: 16px;
    height: 16px;
    padding: 0 4px;
    border-radius: 8px;
    background: var(--c-bg-active);
    font-size: 10px;
    line-height: 16px;
    text-align: center;
  }
  .dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--c-success);
  }
</style>
