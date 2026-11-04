<!-- Settings → Sync: the server connection, which workspaces sync, the log. -->
<script lang="ts">
  import { onMount } from 'svelte';
  import { errorMessage } from '$lib/errors';
  import { i18n, t } from '$lib/i18n';
  import type { SyncStore } from '$lib/stores/sync.svelte';
  import Button from '$lib/ui/Button.svelte';
  import Checkbox from '$lib/ui/Checkbox.svelte';
  import Input from '$lib/ui/Input.svelte';
  import Switch from '$lib/ui/Switch.svelte';
  import Field from '../settings/Field.svelte';

  let { sync }: { sync: SyncStore } = $props();

  let url = $state('');
  let username = $state('');
  let password = $state('');
  let token = $state('');

  onMount(() => {
    void sync.load().then(() => {
      if (sync.connected) {
        void sync.loadWorkspaces();
        void sync.loadLog();
      }
    });
  });

  async function connect(e: SubmitEvent) {
    e.preventDefault();
    const ok = await sync.connect({
      url,
      ...(token.trim() ? { token } : username.trim() ? { username, password } : {}),
    });
    if (ok) {
      password = '';
      token = '';
      await sync.loadWorkspaces();
    }
  }

  const status = $derived.by(() => {
    const s = sync.status;
    if (!s) return '';
    if (s.syncing) return t('sync.syncing');
    if (s.lastError) return errorMessage(s.lastError);
    return s.lastSyncAt
      ? t('sync.lastSync', { time: i18n.formatDateTime(s.lastSyncAt) })
      : t('sync.never');
  });
</script>

<h3 class="title">{t('sync.heading')}</h3>
<p class="intro">{t('sync.intro')}</p>

{#if !sync.connected}
  <form class="connect" onsubmit={connect}>
    <Field label={t('sync.url')}>
      <Input bind:value={url} placeholder={t('sync.urlPlaceholder')} aria-label={t('sync.url')} />
    </Field>
    <Field label={t('sync.username')}>
      <Input bind:value={username} aria-label={t('sync.username')} />
    </Field>
    <Field label={t('sync.password')}>
      <Input bind:value={password} type="password" aria-label={t('sync.password')} />
    </Field>
    <Field label={t('sync.token')} hint={t('sync.tokenHint')}>
      <Input bind:value={token} type="password" aria-label={t('sync.token')} />
    </Field>
    {#if sync.error}<p class="error" role="alert">{sync.error}</p>{/if}
    <div class="actions">
      <Button type="submit" variant="primary" loading={sync.busy} disabled={!url.trim()}
        >{t('sync.connect')}</Button
      >
    </div>
  </form>
{:else}
  <Field
    label={sync.status?.user
      ? t('sync.connectedAs', { url: sync.status.url ?? '', user: sync.status.user })
      : t('sync.connectedTo', { url: sync.status?.url ?? '' })}
    hint={status}
  >
    <div class="row">
      <Button size="sm" onclick={() => void sync.disconnect()}>{t('sync.disconnect')}</Button>
      <Button
        size="sm"
        loading={sync.busy || sync.status?.syncing}
        onclick={() => void sync.syncNow()}>{t('sync.now')}</Button
      >
    </div>
  </Field>
  {#if sync.error}<p class="error" role="alert">{sync.error}</p>{/if}
  {#if sync.status && sync.status.pending > 0}
    <p class="note">{t('sync.pending', { count: sync.status.pending })}</p>
  {/if}
  {#if sync.conflicts.length > 0}
    <Field
      label={t('sync.conflicts')}
      hint={t('sync.conflictsCount', { count: sync.conflicts.length })}
    >
      <div class="row">
        <Button size="sm" variant="primary" onclick={() => (sync.conflictsOpen = true)}
          >{t('sync.resolve')}</Button
        >
      </div>
    </Field>
  {/if}
  <Field label={t('sync.secrets')} hint={t('sync.secretsHint')}>
    <Switch
      label={t('sync.secrets')}
      checked={sync.status?.secrets ?? false}
      onchange={(v) => void sync.setSecrets(v)}
    />
  </Field>

  <h3 class="title">{t('sync.workspaces')}</h3>
  <p class="intro">{t('sync.workspacesHint')}</p>
  {#if sync.workspacesError}<p class="error" role="alert">{sync.workspacesError}</p>{/if}
  <ul class="workspaces" aria-label={t('sync.workspaces')}>
    {#each sync.workspaces as w (w.id)}
      <li>
        <Checkbox
          label={t('sync.syncWorkspace', { name: w.name })}
          checked={w.synced}
          disabled={sync.busy}
          onchange={(v) => void sync.setSynced(w.id, v)}
        />
        <span class="name">{w.name}</span>
        {#if w.local}<span class="badge">{t('sync.here')}</span>{/if}
        {#if w.remote}<span class="badge">{t('sync.there')}</span>{/if}
      </li>
    {:else}
      <li class="empty">{t('sync.noWorkspaces')}</li>
    {/each}
  </ul>

  {#if sync.log.length > 0}
    <h3 class="title">{t('sync.log')}</h3>
    <ul class="log" aria-label={t('sync.log')}>
      {#each sync.log as entry (entry.at)}
        <li class:failed={entry.error}>
          <span class="time">{i18n.formatDateTime(entry.at)}</span>
          {#if entry.error}
            <span>{errorMessage(entry.error)}</span>
          {:else}
            <span>{t('sync.logEntry', { pulled: entry.pulled, pushed: entry.pushed })}</span>
            {#if entry.conflicts}<span>{t('sync.logConflicts', { count: entry.conflicts })}</span
              >{/if}
            {#if entry.rejected}<span>{t('sync.logRejected', { count: entry.rejected })}</span>{/if}
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
{/if}

<style>
  .title {
    margin: var(--space-4) 0 var(--space-2);
    font-size: var(--font-size-sm);
  }
  .title:first-child {
    margin-top: 0;
  }
  .intro,
  .note {
    margin: 0 0 var(--space-2);
    font-size: var(--font-size-sm);
    color: var(--c-fg-muted);
  }
  .error {
    margin: var(--space-2) 0;
    font-size: var(--font-size-sm);
    color: var(--c-danger);
  }
  .actions,
  .row {
    display: flex;
    gap: var(--space-2);
    justify-content: flex-end;
  }
  .actions {
    padding-top: var(--space-3);
  }
  .workspaces,
  .log {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .workspaces li {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-2) 0;
    border-bottom: 1px solid var(--c-border);
  }
  .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .badge {
    padding: 0 var(--space-2);
    border-radius: var(--radius-sm);
    background: var(--c-bg-sunken);
    color: var(--c-fg-muted);
    font-size: var(--font-size-xs);
  }
  .empty {
    color: var(--c-fg-muted);
  }
  .log li {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
    padding: var(--space-1) 0;
    font-size: var(--font-size-sm);
  }
  .log .time {
    color: var(--c-fg-muted);
  }
  .log .failed {
    color: var(--c-danger);
  }
</style>
