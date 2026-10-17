<!-- Ctrl+K / Ctrl+P: jump to requests and run commands. -->
<script lang="ts" module>
  export interface PaletteCommand {
    id: string;
    label: string;
    hint?: string;
    run: () => void;
  }
</script>

<script lang="ts">
  import { onMount } from 'svelte';
  import type { NodeSummary } from '$lib/api/generated/NodeSummary';
  import { t } from '$lib/i18n';
  import type { Services } from '$lib/services.svelte';
  import MethodBadge from '$lib/ui/MethodBadge.svelte';
  import { portal } from '$lib/ui/overlay/portal';
  import { rank } from './fuzzy';

  interface Props {
    services: Services;
    commands: PaletteCommand[];
    onclose: () => void;
  }

  let { services, commands, onclose }: Props = $props();
  let query = $state('');
  let active = $state(0);
  let input: HTMLInputElement | null = $state(null);
  let remote = $state<NodeSummary[]>([]);

  type Entry =
    | { kind: 'request'; id: string; label: string; method: string; path: string }
    | { kind: 'command'; id: string; label: string; hint?: string; run: () => void };

  const local = $derived<Entry[]>(
    services.tree.items
      .filter((i) => !i.isFolder)
      .map((i) => ({
        kind: 'request' as const,
        id: i.id,
        label: i.label,
        method: i.method ?? 'GET',
        path: [...services.tree.pathOf(i.id), i.url ?? ''].filter(Boolean).join(' / '),
      })),
  );
  const commandEntries = $derived<Entry[]>(
    commands.map((c) => ({ kind: 'command' as const, ...c })),
  );

  // Requests in other collections of the workspace come from full-text search.
  $effect(() => {
    const q = query.trim();
    const ws = services.workspace.activeWorkspaceId;
    if (q.length < 2 || !ws) {
      remote = [];
      return;
    }
    const timer = setTimeout(() => {
      void services.transport
        .call('search.nodes', { workspaceId: ws, query: q, limit: 20 })
        .then((r) => (remote = r));
    }, 150);
    return () => clearTimeout(timer);
  });

  const results = $derived.by(() => {
    const q = query.startsWith('>') ? query.slice(1) : query;
    const onlyCommands = query.startsWith('>');
    const pool: Entry[] = onlyCommands ? commandEntries : [...local, ...commandEntries];
    const ranked = rank(
      q,
      pool,
      (e) => (e.kind === 'request' ? `${e.label} ${e.path}` : e.label),
      60,
    ).map((r) => r.item);
    if (!onlyCommands) {
      const seen = new Set(ranked.map((r) => r.id));
      for (const n of remote) {
        if (n.kind === 'request' && !seen.has(n.id))
          ranked.push({
            kind: 'request',
            id: n.id,
            label: n.name,
            method: n.method ?? 'GET',
            path: n.url ?? '',
          });
      }
    }
    return ranked;
  });

  $effect(() => {
    void results;
    active = 0;
  });

  function choose(entry: Entry | undefined) {
    if (!entry) return;
    onclose();
    if (entry.kind === 'request') services.tabs.open(entry.id);
    else entry.run();
  }

  function onKeyDown(e: KeyboardEvent) {
    if (e.key === 'ArrowDown') active = Math.min(active + 1, results.length - 1);
    else if (e.key === 'ArrowUp') active = Math.max(active - 1, 0);
    else if (e.key === 'Enter') choose(results[active]);
    else if (e.key === 'Escape') onclose();
    else if (e.key === 'Tab') {
      // Keep focus in the input.
    } else return;
    e.preventDefault();
  }

  $effect(() => {
    document.getElementById(`palette-item-${active}`)?.scrollIntoView({ block: 'nearest' });
  });

  onMount(() => {
    const previous = document.activeElement as HTMLElement | null;
    queueMicrotask(() => input?.focus());
    return () => previous?.focus?.();
  });
</script>

<div
  use:portal
  class="backdrop"
  role="presentation"
  onpointerdown={(e) => e.target === e.currentTarget && onclose()}
>
  <div class="palette" role="dialog" aria-modal="true" aria-label={t('palette.title')}>
    <input
      bind:this={input}
      bind:value={query}
      class="input"
      role="combobox"
      aria-expanded="true"
      aria-controls="palette-list"
      aria-activedescendant="palette-item-{active}"
      aria-label={t('palette.title')}
      placeholder={t('palette.placeholder')}
      spellcheck="false"
      autocomplete="off"
      onkeydown={onKeyDown}
    />
    <div id="palette-list" class="list" role="listbox" aria-label={t('palette.title')}>
      {#each results as entry, i (entry.kind + entry.id)}
        <div
          id="palette-item-{i}"
          class="item"
          class:active={i === active}
          role="option"
          aria-selected={i === active}
          tabindex="-1"
          onpointermove={() => (active = i)}
          onclick={() => choose(entry)}
          onkeydown={() => {}}
        >
          {#if entry.kind === 'request'}
            <MethodBadge method={entry.method} />
            <span class="label">{entry.label}</span>
            <span class="hint">{entry.path}</span>
          {:else}
            <span class="cmd">›</span>
            <span class="label">{entry.label}</span>
            {#if entry.hint}<span class="hint">{entry.hint}</span>{/if}
          {/if}
        </div>
      {:else}
        <div class="empty">{t('palette.empty')}</div>
      {/each}
    </div>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 920;
    background: var(--c-overlay);
    display: flex;
    justify-content: center;
    align-items: flex-start;
    padding-top: 12vh;
  }
  .palette {
    width: min(640px, 92vw);
    background: var(--c-bg-elevated);
    border: 1px solid var(--c-border-strong);
    border-radius: var(--radius-lg);
    box-shadow: 0 20px 60px var(--c-shadow);
    overflow: hidden;
  }
  .input {
    width: 100%;
    height: 44px;
    padding: 0 var(--space-4);
    border: 0;
    border-bottom: 1px solid var(--c-border);
    background: transparent;
    color: var(--c-fg);
    font-size: 15px;
    outline: none;
  }
  .list {
    max-height: 50vh;
    overflow: auto;
    padding: 4px;
  }
  .item {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    height: 32px;
    padding: 0 var(--space-2);
    border-radius: var(--radius-sm);
  }
  .item.active {
    background: var(--c-bg-selected);
  }
  .cmd {
    min-width: 34px;
    text-align: right;
    color: var(--c-fg-subtle);
  }
  .label {
    white-space: nowrap;
  }
  .hint {
    margin-left: auto;
    color: var(--c-fg-subtle);
    font-size: var(--font-size-xs);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 55%;
  }
  .empty {
    padding: var(--space-4);
    text-align: center;
    color: var(--c-fg-subtle);
  }
</style>
