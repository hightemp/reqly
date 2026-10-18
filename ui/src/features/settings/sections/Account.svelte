<!-- The signed-in user of a multi-user server: name, password, API tokens. -->
<script lang="ts">
  import { onMount } from 'svelte';
  import type { ApiToken } from '$lib/api/generated/ApiToken';
  import { errorMessage } from '$lib/errors';
  import { t } from '$lib/i18n';
  import type { Services } from '$lib/services.svelte';
  import Button from '$lib/ui/Button.svelte';
  import Input from '$lib/ui/Input.svelte';
  import { dialogs } from '$lib/ui/overlay/dialogs.svelte';
  import { toasts } from '$lib/ui/overlay/toasts.svelte';
  import Field from '../Field.svelte';

  let { services }: { services: Services } = $props();
  const account = $derived(services.account);
  const transport = $derived(services.transport);

  // svelte-ignore state_referenced_locally
  let displayName = $state(account.user?.displayName ?? '');
  let current = $state('');
  let next = $state('');
  let repeat = $state('');
  let tokens = $state<ApiToken[]>([]);
  let secret = $state<string | null>(null);
  let busy = $state(false);

  const date = (ms: number | null) => (ms === null ? '—' : new Date(ms).toLocaleDateString());

  async function run(action: () => Promise<void>) {
    busy = true;
    try {
      await action();
    } catch (e) {
      toasts.error(errorMessage(e));
    } finally {
      busy = false;
    }
  }

  const loadTokens = () =>
    run(async () => {
      tokens = await transport.call('account.tokens', {});
    });

  const saveName = () =>
    run(async () => {
      account.user = await transport.call('account.update', { displayName });
      toasts.show({ message: t('account.saved'), kind: 'success' });
    });

  const changePassword = () =>
    run(async () => {
      if (next !== repeat) {
        toasts.error(t('account.passwordMismatch'));
        return;
      }
      await transport.call('account.changePassword', { current, new: next });
      // Every session ended, this one too.
      location.reload();
    });

  async function createToken() {
    const name = await dialogs.prompt({
      title: t('account.newToken'),
      label: t('actions.name'),
      validate: (v) => (v.trim() ? null : t('actions.nameRequired')),
    });
    if (!name?.trim()) return;
    await run(async () => {
      const out = await transport.call('account.createToken', { name: name.trim() });
      secret = out.secret;
      tokens = [...tokens, out.token];
    });
  }

  async function deleteToken(token: ApiToken) {
    const ok = await dialogs.confirm({
      title: t('account.deleteToken', { name: token.name }),
      message: t('account.deleteTokenMessage'),
      confirmLabel: t('tree.delete'),
      danger: true,
    });
    if (!ok) return;
    await run(async () => {
      await transport.call('account.deleteToken', { tokenId: token.id });
      tokens = tokens.filter((t) => t.id !== token.id);
    });
  }

  async function copySecret() {
    if (!secret) return;
    await navigator.clipboard.writeText(secret);
    toasts.show({ message: t('account.copied'), kind: 'success' });
  }

  onMount(() => void loadTokens());
</script>

{#if account.user}
  <Field label={t('login.username')}>
    <span class="mono">{account.user.username}</span>
  </Field>
  <Field label={t('account.displayName')}>
    <div class="row">
      <Input bind:value={displayName} aria-label={t('account.displayName')} />
      <Button size="sm" disabled={busy || !displayName.trim()} onclick={() => void saveName()}
        >{t('account.save')}</Button
      >
    </div>
  </Field>
  <Field label={t('account.password')} hint={t('account.passwordHint')}>
    <form
      class="stack"
      onsubmit={(e) => {
        e.preventDefault();
        void changePassword();
      }}
    >
      <Input
        type="password"
        bind:value={current}
        autocomplete="current-password"
        placeholder={t('account.currentPassword')}
        aria-label={t('account.currentPassword')}
      />
      <Input
        type="password"
        bind:value={next}
        autocomplete="new-password"
        placeholder={t('account.newPassword')}
        aria-label={t('account.newPassword')}
      />
      <Input
        type="password"
        bind:value={repeat}
        autocomplete="new-password"
        placeholder={t('account.repeatPassword')}
        aria-label={t('account.repeatPassword')}
      />
      <Button size="sm" type="submit" disabled={busy || !current || !next}
        >{t('account.changePassword')}</Button
      >
    </form>
  </Field>

  <h3>{t('account.tokens')}</h3>
  <p class="hint">{t('account.tokensHint')}</p>
  {#if secret}
    <div class="secret" role="status">
      <p>{t('account.secretOnce')}</p>
      <div class="row">
        <code class="mono selectable">{secret}</code>
        <Button size="sm" onclick={() => void copySecret()}>{t('account.copy')}</Button>
      </div>
    </div>
  {/if}
  {#if tokens.length}
    <table>
      <thead>
        <tr>
          <th>{t('actions.name')}</th>
          <th>{t('account.created')}</th>
          <th>{t('account.lastUsed')}</th>
          <th>{t('account.expires')}</th>
          <th></th>
        </tr>
      </thead>
      <tbody>
        {#each tokens as token (token.id)}
          <tr>
            <td>{token.name}</td>
            <td>{date(token.createdAt)}</td>
            <td>{date(token.lastUsedAt)}</td>
            <td>{date(token.expiresAt)}</td>
            <td class="end">
              <Button size="sm" variant="ghost" onclick={() => void deleteToken(token)}
                >{t('tree.delete')}</Button
              >
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
  <Button size="sm" onclick={() => void createToken()}>{t('account.newToken')}</Button>
{/if}

<style>
  .row {
    display: flex;
    gap: var(--space-2);
    align-items: center;
  }
  .stack {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    align-items: flex-start;
  }
  h3 {
    margin-top: var(--space-4);
    font-size: var(--font-size);
    font-weight: 600;
  }
  .hint {
    color: var(--c-fg-subtle);
    font-size: var(--font-size-sm);
    margin: var(--space-1) 0 var(--space-2);
  }
  .mono {
    font-family: var(--font-mono);
    font-size: var(--font-size-sm);
    word-break: break-all;
  }
  .secret {
    padding: var(--space-2);
    margin-bottom: var(--space-2);
    border: 1px solid var(--c-border);
    border-radius: var(--radius-sm);
    background: var(--c-bg-sunken);
  }
  table {
    width: 100%;
    border-collapse: collapse;
    margin-bottom: var(--space-2);
    font-size: var(--font-size-sm);
  }
  th,
  td {
    text-align: left;
    padding: var(--space-1) var(--space-2);
    border-bottom: 1px solid var(--c-border);
  }
  th {
    color: var(--c-fg-subtle);
    font-weight: 500;
  }
  .end {
    text-align: right;
  }
</style>
