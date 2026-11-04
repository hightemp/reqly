<!-- Deleted on one side, changed on the other: keep mine or take the server's. -->
<script lang="ts">
  import type { SyncConflict } from '$lib/api/generated/SyncConflict';
  import { t } from '$lib/i18n';
  import type { SyncStore } from '$lib/stores/sync.svelte';
  import Button from '$lib/ui/Button.svelte';
  import Modal from '$lib/ui/overlay/Modal.svelte';

  let { sync }: { sync: SyncStore } = $props();
  // i18n-dynamic: sync.kinds.

  function name(c: SyncConflict): string {
    const n = c.local.fields.name ?? c.remote.fields.name;
    return typeof n === 'string' && n ? n : t('sync.unnamed');
  }
</script>

<Modal title={t('sync.conflictsTitle')} width="600px" onclose={() => (sync.conflictsOpen = false)}>
  <p class="intro">{t('sync.conflictsIntro')}</p>
  {#if sync.error}<p class="error" role="alert">{sync.error}</p>{/if}
  <ul class="list" aria-label={t('sync.conflicts')}>
    {#each sync.conflicts as c (c.local.id)}
      <li>
        <div class="text">
          <span class="name">{name(c)}</span>
          <span class="what"
            >{t(`sync.kinds.${c.local.kind}`)} · {c.remote.deleted
              ? t('sync.deletedThere')
              : t('sync.deletedHere')}</span
          >
        </div>
        <div class="buttons">
          <Button size="sm" disabled={sync.busy} onclick={() => void sync.resolve(c, 'takeRemote')}
            >{t('sync.takeRemote')}</Button
          >
          <Button
            size="sm"
            variant="primary"
            disabled={sync.busy}
            onclick={() => void sync.resolve(c, 'keepLocal')}>{t('sync.keepLocal')}</Button
          >
        </div>
      </li>
    {/each}
  </ul>
</Modal>

<style>
  .intro {
    margin: 0 0 var(--space-3);
    color: var(--c-fg-muted);
    font-size: var(--font-size-sm);
  }
  .error {
    color: var(--c-danger);
    font-size: var(--font-size-sm);
  }
  .list {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .list li {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-2) 0;
    border-bottom: 1px solid var(--c-border);
  }
  .text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .what {
    font-size: var(--font-size-xs);
    color: var(--c-fg-muted);
  }
  .buttons {
    display: flex;
    gap: var(--space-2);
    flex-wrap: wrap;
    justify-content: flex-end;
  }
</style>
