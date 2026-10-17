<!-- Every request sent in the workspace, newest first; opens the request at that response. -->
<script lang="ts">
  import type { ResponseSummary } from '$lib/api/generated/ResponseSummary';
  import { i18n, t } from '$lib/i18n';
  import type { Services } from '$lib/services.svelte';
  import EmptyState from '$lib/ui/EmptyState.svelte';
  import Input from '$lib/ui/Input.svelte';
  import MethodBadge from '$lib/ui/MethodBadge.svelte';
  import Modal from '$lib/ui/overlay/Modal.svelte';
  import StatusBadge from '$lib/ui/StatusBadge.svelte';

  let { services, onclose }: { services: Services; onclose: () => void } = $props();
  let filter = $state('');
  let rows = $state<ResponseSummary[]>([]);

  $effect(() => {
    const workspaceId = services.workspace.activeWorkspaceId;
    const f = filter.trim();
    if (!workspaceId) return;
    const timer = setTimeout(() => {
      void services.transport
        .call('history.workspace', { workspaceId, filter: f || undefined, limit: 300 })
        .then((r) => (rows = r));
    }, 150);
    return () => clearTimeout(timer);
  });

  async function open(row: ResponseSummary) {
    onclose();
    services.tabs.open(row.requestId);
    await services.responses.load(row.requestId);
    await services.responses.select(row.requestId, row.id);
  }
</script>

<Modal title={t('log.title')} width="min(960px, 96vw)" {onclose}>
  <Input
    value={filter}
    placeholder={t('log.filter')}
    aria-label={t('log.filter')}
    oninput={(e) => (filter = e.currentTarget.value)}
  />
  {#if rows.length === 0}
    <div class="empty"><EmptyState title={t('log.empty')} /></div>
  {:else}
    <div class="list">
      {#each rows as row (row.id)}
        <button type="button" class="row" onclick={() => void open(row)}>
          <span class="when"
            >{i18n.formatDateTime(row.createdAt, { dateStyle: 'short', timeStyle: 'medium' })}</span
          >
          <MethodBadge method={row.method} />
          <span class="url">{row.url}</span>
          {#if row.errorCode}
            <StatusBadge status={null} text={t('response.error')} />
          {:else}
            <StatusBadge status={row.status} />
          {/if}
          <span class="ms">{row.totalMs != null ? i18n.formatDuration(row.totalMs) : ''}</span>
        </button>
      {/each}
    </div>
  {/if}
</Modal>

<style>
  .empty {
    height: 200px;
  }
  .list {
    margin-top: var(--space-2);
    max-height: 60vh;
    overflow: auto;
  }
  .row {
    width: 100%;
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: 4px var(--space-1);
    border-bottom: 1px solid var(--c-border);
    text-align: left;
  }
  .row:hover {
    background: var(--c-bg-hover);
  }
  .when {
    color: var(--c-fg-subtle);
    font-size: var(--font-size-xs);
    white-space: nowrap;
  }
  .url {
    flex: 1;
    font-family: var(--font-mono);
    font-size: var(--font-size-sm);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .ms {
    width: 70px;
    text-align: right;
    color: var(--c-fg-muted);
    font-size: var(--font-size-sm);
  }
</style>
