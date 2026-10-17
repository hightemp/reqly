<!-- Runtime variables set by scripts for this session (highest priority). -->
<script lang="ts">
  import { onDestroy } from 'svelte';
  import { errorMessage } from '$lib/errors';
  import { t } from '$lib/i18n';
  import type { Services } from '$lib/services.svelte';
  import Button from '$lib/ui/Button.svelte';
  import EmptyState from '$lib/ui/EmptyState.svelte';
  import { toasts } from '$lib/ui/overlay/toasts.svelte';

  let { services }: { services: Services } = $props();
  let variables = $state<Record<string, unknown>>({});
  const collectionId = $derived(services.workspace.activeCollectionId);

  async function load() {
    if (!collectionId) return;
    try {
      variables = (await services.transport.call('variables.runtime', { collectionId })).variables;
    } catch (e) {
      toasts.error(errorMessage(e));
    }
  }

  $effect(() => {
    void collectionId;
    void load();
  });

  const off = services.transport.subscribe((e) => {
    if (e.topic === 'variables.runtimeChanged') {
      const p = e.payload as { collectionId?: string };
      if (p.collectionId === collectionId) void load();
    }
  });
  onDestroy(off);

  async function clear() {
    if (!collectionId) return;
    await services.transport.call('variables.clearRuntime', { collectionId });
    variables = {};
  }

  const entries = $derived(Object.entries(variables));
  const show = (v: unknown) => (typeof v === 'string' ? v : JSON.stringify(v));
</script>

<div class="runtime">
  <header>
    <h3>{t('environments.runtime')}</h3>
    <span class="spacer"></span>
    <Button size="sm" disabled={entries.length === 0} onclick={() => void clear()}
      >{t('environments.clearRuntime')}</Button
    >
  </header>
  <p class="hint">{t('environments.runtimeHint')}</p>
  {#if entries.length === 0}
    <EmptyState title={t('environments.noRuntime')} />
  {:else}
    <table class="selectable">
      <tbody>
        {#each entries as [name, value] (name)}
          <tr><th>{name}</th><td>{show(value)}</td></tr>
        {/each}
      </tbody>
    </table>
  {/if}
</div>

<style>
  .runtime {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }
  header {
    display: flex;
    align-items: center;
  }
  h3 {
    font-size: var(--font-size-md, 14px);
    font-weight: 600;
  }
  .spacer {
    flex: 1;
  }
  .hint {
    color: var(--c-fg-subtle);
    font-size: var(--font-size-xs);
  }
  table {
    border-collapse: collapse;
    font-size: var(--font-size-sm);
  }
  th {
    text-align: left;
    padding: var(--space-1) var(--space-3) var(--space-1) 0;
    color: var(--c-fg-muted);
    font-weight: 500;
    vertical-align: top;
  }
  td {
    font-family: var(--font-mono);
    font-size: var(--font-size-xs);
    word-break: break-all;
  }
</style>
