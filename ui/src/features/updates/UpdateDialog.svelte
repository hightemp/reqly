<!-- A newer release: its notes, then update in place or download it. -->
<script lang="ts">
  import { i18n, t } from '$lib/i18n';
  import type { UpdatesStore } from '$lib/stores/updates.svelte';
  import Button from '$lib/ui/Button.svelte';
  import Modal from '$lib/ui/overlay/Modal.svelte';
  import ProgressBar from '$lib/ui/ProgressBar.svelte';

  interface Props {
    updates: UpdatesStore;
    /** Opens a web page outside the app. */
    onopen: (url: string) => void;
  }

  let { updates, onopen }: Props = $props();
  const release = $derived(updates.release);
  const progress = $derived(
    updates.progress?.total ? updates.progress.downloaded / updates.progress.total : undefined,
  );
  const busy = $derived(updates.status === 'downloading' || updates.status === 'ready');

  function close() {
    if (updates.status !== 'downloading') updates.dialogOpen = false;
  }
</script>

<Modal title={t('updates.title')} width="560px" onclose={close}>
  {#if release}
    <p class="lead">
      {t('updates.available', { version: release.version, current: updates.current ?? '' })}
    </p>
    {#if release.publishedAt}
      <p class="muted">
        {t('updates.published', {
          date: i18n.formatDateTime(new Date(release.publishedAt), { dateStyle: 'medium' }),
        })}
      </p>
    {/if}
    <div class="notes-head">
      <h3 class="heading">{t('updates.notes')}</h3>
      <button type="button" class="link" onclick={() => onopen(release.url)}
        >{t('updates.openRelease')}</button
      >
    </div>
    <pre class="notes selectable">{release.notes.trim() || t('updates.noNotes')}</pre>
    {#if updates.status === 'downloading'}
      <p>{t('updates.downloading')}</p>
      <ProgressBar value={progress} label={t('updates.progress')} />
    {:else if updates.status === 'ready'}
      <p role="status">{t('updates.ready')}</p>
    {:else if updates.status === 'error' && updates.error}
      <p class="error" role="alert">{updates.error}</p>
    {/if}
    {#if updates.serverCommand}
      <p class="muted">{t('updates.server')}</p>
      <pre class="command selectable">{updates.serverCommand}</pre>
    {:else if !updates.canInstall}
      <p class="muted">{t('updates.manual')}</p>
    {/if}
  {/if}

  {#snippet footer()}
    {#if updates.status === 'ready'}
      <Button onclick={close}>{t('updates.later')}</Button>
      <Button variant="primary" onclick={() => void updates.restart()}
        >{t('updates.restart')}</Button
      >
    {:else}
      <Button disabled={busy} onclick={() => void updates.skip()}>{t('updates.skip')}</Button>
      <span class="spacer"></span>
      <Button disabled={updates.status === 'downloading'} onclick={close}
        >{t('updates.later')}</Button
      >
      {#if updates.canInstall}
        <Button
          variant="primary"
          loading={updates.status === 'downloading'}
          onclick={() => void updates.install()}>{t('updates.install')}</Button
        >
      {:else if updates.downloadUrl}
        <Button variant="primary" onclick={() => onopen(updates.downloadUrl!)}
          >{t('updates.download')}</Button
        >
      {/if}
    {/if}
  {/snippet}
</Modal>

<style>
  .lead {
    margin: 0 0 var(--space-1);
    font-weight: 600;
  }
  .muted {
    margin: 0 0 var(--space-2);
    color: var(--c-fg-muted);
    font-size: var(--font-size-sm);
  }
  .notes-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
  }
  .link {
    color: var(--c-accent);
    font-size: var(--font-size-sm);
  }
  .link:hover {
    text-decoration: underline;
  }
  .heading {
    margin: var(--space-3) 0 var(--space-2);
    font-size: var(--font-size-sm);
  }
  .notes {
    max-height: 280px;
    overflow: auto;
    margin: 0 0 var(--space-3);
    padding: var(--space-2) var(--space-3);
    background: var(--c-bg-sunken);
    border: 1px solid var(--c-border);
    border-radius: var(--radius-sm);
    font-family: var(--font-mono);
    font-size: var(--font-size-sm);
    white-space: pre-wrap;
    word-break: break-word;
  }
  .command {
    margin: 0 0 var(--space-2);
    padding: var(--space-2) var(--space-3);
    background: var(--c-bg-sunken);
    border-radius: var(--radius-sm);
    font-family: var(--font-mono);
    font-size: var(--font-size-sm);
    overflow-x: auto;
  }
  .error {
    color: var(--c-danger);
  }
  .spacer {
    flex: 1;
  }
</style>
