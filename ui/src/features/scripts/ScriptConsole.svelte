<!-- Console output of a response's scripts. -->
<script lang="ts">
  import type { LogEntry } from '$lib/api/generated/LogEntry';
  import { t } from '$lib/i18n';
  import EmptyState from '$lib/ui/EmptyState.svelte';

  let { logs }: { logs: LogEntry[] } = $props();
</script>

{#if logs.length === 0}
  <EmptyState title={t('scripts.noLogs')} />
{:else}
  <div class="console selectable" role="log" aria-label={t('scripts.console')}>
    {#each logs as entry, i (i)}
      {#if i === 0 || logs[i - 1]?.source !== entry.source}
        <div class="source">{entry.source}</div>
      {/if}
      <pre class="line" data-level={entry.level}>{entry.text}</pre>
    {/each}
  </div>
{/if}

<style>
  .console {
    font-family: var(--font-mono);
    font-size: var(--font-size-xs);
  }
  .source {
    margin: var(--space-2) 0 var(--space-1);
    color: var(--c-fg-subtle);
    font-family: var(--font-sans, inherit);
  }
  .line {
    margin: 0;
    padding: 2px var(--space-2);
    white-space: pre-wrap;
    word-break: break-word;
    border-left: 2px solid transparent;
  }
  .line[data-level='warn'] {
    color: var(--c-warning, #b45309);
    border-left-color: var(--c-warning, #f59e0b);
  }
  .line[data-level='error'] {
    color: var(--c-danger);
    border-left-color: var(--c-danger);
  }
  .line[data-level='debug'],
  .line[data-level='info'] {
    color: var(--c-fg-muted);
  }
</style>
