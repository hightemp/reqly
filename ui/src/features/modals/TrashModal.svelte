<!-- Trash of the active collection: restore or delete forever. -->
<script lang="ts">
  import { onMount } from 'svelte';
  import type { Node } from '$lib/api/generated/Node';
  import { i18n, t } from '$lib/i18n';
  import type { Services } from '$lib/services.svelte';
  import Button from '$lib/ui/Button.svelte';
  import EmptyState from '$lib/ui/EmptyState.svelte';
  import Icon from '$lib/ui/Icon.svelte';
  import { methodOf, protocolOf } from '$lib/ui/badges';
  import MethodBadge from '$lib/ui/MethodBadge.svelte';
  import { dialogs } from '$lib/ui/overlay/dialogs.svelte';
  import Modal from '$lib/ui/overlay/Modal.svelte';

  let { services, onclose }: { services: Services; onclose: () => void } = $props();
  let items = $state<Node[]>([]);
  const collectionId = $derived(services.workspace.activeCollectionId);

  async function load() {
    if (collectionId) items = await services.transport.call('trash.list', { collectionId });
  }

  async function restore(ids: string[]) {
    await services.transport.call('tree.restore', { ids });
    await load();
  }

  async function purge(ids: string[]) {
    const ok = await dialogs.confirm({
      title: t('trash.purgeTitle', { count: ids.length }),
      message: t('trash.purgeMessage'),
      danger: true,
      confirmLabel: t('trash.purge'),
    });
    if (!ok) return;
    await services.transport.call('trash.purge', { ids });
    await load();
  }

  onMount(() => void load());
</script>

<Modal title={t('trash.title')} width="min(720px, 94vw)" {onclose}>
  {#if items.length === 0}
    <div class="empty"><EmptyState title={t('trash.empty')} /></div>
  {:else}
    <ul class="list">
      {#each items as n (n.id)}
        <li class="row">
          {#if n.data.kind === 'folder'}
            <span class="icon"><Icon name="folder" size={14} /></span>
          {:else}
            <MethodBadge method={methodOf(n.data)} protocol={protocolOf(n.data)} />
          {/if}
          <span class="name">{n.name}</span>
          <span class="when"
            >{n.deletedAt
              ? i18n.formatDateTime(n.deletedAt, { dateStyle: 'short', timeStyle: 'short' })
              : ''}</span
          >
          <Button size="sm" variant="ghost" onclick={() => void restore([n.id])}
            >{t('trash.restore')}</Button
          >
          <Button size="sm" variant="ghost" onclick={() => void purge([n.id])}
            >{t('trash.purge')}</Button
          >
        </li>
      {/each}
    </ul>
  {/if}
  {#snippet footer()}
    <Button
      variant="danger"
      disabled={items.length === 0}
      onclick={() => void purge(items.map((i) => i.id))}>{t('trash.emptyAll')}</Button
    >
    <Button onclick={onclose}>{t('common.close')}</Button>
  {/snippet}
</Modal>

<style>
  .empty {
    height: 200px;
  }
  .list {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: 4px 0;
    border-bottom: 1px solid var(--c-border);
  }
  .icon {
    min-width: 34px;
    display: flex;
    justify-content: flex-end;
    color: var(--c-fg-subtle);
  }
  .name {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .when {
    color: var(--c-fg-subtle);
    font-size: var(--font-size-xs);
  }
</style>
