<!-- Cookie jar of the active workspace: search, edit, add, delete, clear. -->
<script lang="ts">
  import { onMount } from 'svelte';
  import type { Cookie } from '$lib/api/generated/Cookie';
  import { errorMessage } from '$lib/errors';
  import { i18n, t } from '$lib/i18n';
  import type { Services } from '$lib/services.svelte';
  import Button from '$lib/ui/Button.svelte';
  import Checkbox from '$lib/ui/Checkbox.svelte';
  import EmptyState from '$lib/ui/EmptyState.svelte';
  import IconButton from '$lib/ui/IconButton.svelte';
  import Input from '$lib/ui/Input.svelte';
  import { dialogs } from '$lib/ui/overlay/dialogs.svelte';
  import Modal from '$lib/ui/overlay/Modal.svelte';
  import { toasts } from '$lib/ui/overlay/toasts.svelte';

  let { services, onclose }: { services: Services; onclose: () => void } = $props();
  const workspaceId = $derived(services.workspace.activeWorkspaceId);
  let cookies = $state<Cookie[]>([]);
  let query = $state('');

  const visible = $derived(
    cookies.filter(
      (c) =>
        !query || `${c.domain} ${c.name} ${c.value}`.toLowerCase().includes(query.toLowerCase()),
    ),
  );

  async function load() {
    if (workspaceId) cookies = await services.transport.call('cookies.list', { workspaceId });
  }

  async function save(cookie: Cookie) {
    if (!workspaceId || !cookie.domain || !cookie.name) return;
    try {
      await services.transport.call('cookies.upsert', { workspaceId, cookie });
      await load();
    } catch (e) {
      toasts.error(errorMessage(e));
    }
  }

  function patch(c: Cookie, p: Partial<Cookie>) {
    const next = { ...c, ...p };
    cookies = cookies.map((x) => (x.id === c.id ? next : x));
    return next;
  }

  async function remove(c: Cookie) {
    if (!workspaceId) return;
    await services.transport.call('cookies.delete', { workspaceId, ids: [c.id] });
    await load();
  }

  async function clearAll() {
    if (!workspaceId) return;
    const ok = await dialogs.confirm({
      title: t('cookieManager.clearTitle'),
      danger: true,
      confirmLabel: t('tree.delete'),
    });
    if (ok) {
      await services.transport.call('cookies.clear', { workspaceId });
      await load();
    }
  }

  function add() {
    cookies = [
      {
        id: crypto.randomUUID(),
        domain: '',
        includeSubdomains: false,
        path: '/',
        secure: false,
        httpOnly: false,
        expires: null,
        name: '',
        value: '',
      },
      ...cookies,
    ];
  }

  const expiresText = (c: Cookie) =>
    c.expires
      ? i18n.formatDateTime(c.expires * 1000, { dateStyle: 'short', timeStyle: 'short' })
      : t('cookies.session');

  onMount(() => {
    void load();
    return services.transport.subscribe((e) => e.topic === 'cookies.changed' && void load());
  });
</script>

<Modal title={t('cookieManager.title')} width="min(960px, 96vw)" {onclose}>
  <div class="toolbar">
    <Input
      value={query}
      placeholder={t('cookieManager.search')}
      aria-label={t('cookieManager.search')}
      oninput={(e) => (query = e.currentTarget.value)}
    />
    <Button size="sm" onclick={add}>{t('cookieManager.add')}</Button>
    <Button
      size="sm"
      variant="ghost"
      disabled={cookies.length === 0}
      onclick={() => void clearAll()}>{t('cookieManager.clear')}</Button
    >
  </div>
  {#if visible.length === 0}
    <div class="empty"><EmptyState title={t('cookieManager.empty')} /></div>
  {:else}
    <table class="table">
      <thead>
        <tr>
          <th>{t('cookies.domain')}</th>
          <th>{t('cookies.name')}</th>
          <th>{t('cookies.value')}</th>
          <th>{t('cookies.path')}</th>
          <th>{t('cookies.expires')}</th>
          <th>{t('cookieManager.secure')}</th>
          <th>{t('cookieManager.httpOnly')}</th>
          <th></th>
        </tr>
      </thead>
      <tbody>
        {#each visible as c (c.id)}
          <tr>
            <td
              ><Input
                mono
                value={c.domain}
                aria-label={t('cookies.domain')}
                onchange={(e) => void save(patch(c, { domain: e.currentTarget.value.trim() }))}
              /></td
            >
            <td
              ><Input
                mono
                value={c.name}
                aria-label={t('cookies.name')}
                onchange={(e) => void save(patch(c, { name: e.currentTarget.value.trim() }))}
              /></td
            >
            <td
              ><Input
                mono
                value={c.value}
                aria-label={t('cookies.value')}
                onchange={(e) => void save(patch(c, { value: e.currentTarget.value }))}
              /></td
            >
            <td
              ><Input
                mono
                value={c.path}
                aria-label={t('cookies.path')}
                onchange={(e) => void save(patch(c, { path: e.currentTarget.value.trim() || '/' }))}
              /></td
            >
            <td class="muted">{expiresText(c)}</td>
            <td
              ><Checkbox
                label={t('cookieManager.secure')}
                checked={c.secure}
                onchange={(v) => void save(patch(c, { secure: v }))}
              /></td
            >
            <td
              ><Checkbox
                label={t('cookieManager.httpOnly')}
                checked={c.httpOnly}
                onchange={(v) => void save(patch(c, { httpOnly: v }))}
              /></td
            >
            <td
              ><IconButton
                icon="trash"
                size={14}
                label={t('tree.delete')}
                onclick={() => void remove(c)}
              /></td
            >
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
</Modal>

<style>
  .toolbar {
    display: flex;
    gap: var(--space-2);
    margin-bottom: var(--space-3);
  }
  .empty {
    height: 200px;
  }
  .table {
    width: 100%;
    border-collapse: collapse;
  }
  th {
    text-align: left;
    font-weight: 500;
    color: var(--c-fg-muted);
    font-size: var(--font-size-sm);
    padding: 4px;
  }
  td {
    padding: 2px 4px;
  }
  .muted {
    color: var(--c-fg-subtle);
    font-size: var(--font-size-sm);
    white-space: nowrap;
  }
</style>
