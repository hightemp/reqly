<!-- Sign-in for the server build: an access token (`token` mode) or a user
     name and password, with registration when the server allows it (`multi`). -->
<script lang="ts">
  import { onMount } from 'svelte';
  import { t, type Key } from '$lib/i18n';
  import Button from '$lib/ui/Button.svelte';
  import Input from '$lib/ui/Input.svelte';

  import {
    forgetInvite,
    inviteToken,
    invitePreview,
    type AuthStatus,
    type InvitePreview,
  } from './status';

  let {
    status,
    onsuccess,
  }: { status: Pick<AuthStatus, 'mode' | 'registration'>; onsuccess: () => void } = $props();
  const multi = $derived(status.mode === 'multi');
  const invite = inviteToken();
  let preview = $state<InvitePreview | null>(null);
  let inviteChecked = $state(false);
  const canRegister = $derived(
    multi &&
      (status.registration === 'open' || (status.registration === 'invite' && preview !== null)),
  );

  let registering = $state(false);
  let token = $state('');
  let username = $state('');
  let password = $state('');
  let displayName = $state('');
  let error = $state<string | null>(null);
  let busy = $state(false);
  let input: HTMLInputElement | null = $state(null);

  const ROLES = {
    owner: 'members.owner',
    editor: 'members.editor',
    viewer: 'members.viewer',
  } as const;

  const ERRORS: Record<string, Key> = {
    'auth.invalid': 'login.invalidPassword',
    'auth.locked': 'login.locked',
    'auth.disabled': 'login.disabled',
    'auth.rate_limited': 'login.tooMany',
    'auth.registration_closed': 'login.registrationClosed',
    'user.exists': 'login.userExists',
    'user.weak_password': 'login.weakPassword',
    'user.invalid_username': 'login.invalidUsername',
    'invite.invalid': 'login.inviteInvalid',
  };

  function messageFor(code: string | undefined, status: number): string {
    if (!multi) return status === 429 ? t('login.tooMany') : t('login.invalid');
    const key = code ? ERRORS[code] : undefined;
    return key ? t(key) : t('login.failed');
  }

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    const body = !multi
      ? { token: token.trim() }
      : registering
        ? {
            username: username.trim(),
            password,
            displayName: displayName.trim(),
            ...(preview && invite ? { invite } : {}),
          }
        : { username: username.trim(), password };
    if (multi ? !username.trim() || !password : !token.trim()) return;
    busy = true;
    error = null;
    try {
      const res = await fetch(registering ? '/api/v1/auth/register' : '/api/v1/auth/login', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        credentials: 'same-origin',
        body: JSON.stringify(body),
      });
      if (res.ok) {
        // A new account has joined already; signing in joins after loading.
        if (registering) forgetInvite();
        onsuccess();
        return;
      }
      const data = (await res.json().catch(() => null)) as { code?: string } | null;
      error = messageFor(data?.code, res.status);
    } catch {
      error = t('errors.transport.network');
    } finally {
      busy = false;
    }
  }

  function toggleMode() {
    registering = !registering;
    error = null;
  }

  onMount(() => {
    input?.focus();
    if (multi && invite)
      void invitePreview(invite).then((p) => {
        preview = p;
        inviteChecked = true;
        if (p && status.registration !== 'admin') registering = true;
      });
  });
</script>

<div class="screen">
  <form class="card" onsubmit={submit}>
    <img class="logo" src="/favicon.svg" alt="" width="48" height="48" />
    <h1>{registering ? t('login.registerTitle') : t('login.title')}</h1>
    {#if preview}
      <p class="invite" role="status">
        {preview.workspaceName
          ? t('login.invitedTo', {
              name: preview.invitedBy,
              workspace: preview.workspaceName,
              role: t(ROLES[preview.role ?? 'viewer']),
            })
          : t('login.invitedServer', { name: preview.invitedBy })}
      </p>
    {:else if invite && inviteChecked}
      <p class="error" role="alert">{t('login.inviteInvalid')}</p>
    {/if}
    <p class="hint">{multi ? t('login.hintUser') : t('login.hint')}</p>
    {#if multi}
      <Input
        bind:inputRef={input}
        bind:value={username}
        autocomplete="username"
        aria-label={t('login.username')}
        placeholder={t('login.username')}
        invalid={error !== null}
      />
      {#if registering}
        <Input
          bind:value={displayName}
          autocomplete="name"
          aria-label={t('login.displayName')}
          placeholder={t('login.displayName')}
        />
      {/if}
      <Input
        bind:value={password}
        type="password"
        autocomplete={registering ? 'new-password' : 'current-password'}
        aria-label={t('login.password')}
        placeholder={t('login.password')}
        invalid={error !== null}
      />
    {:else}
      <Input
        bind:inputRef={input}
        bind:value={token}
        type="password"
        aria-label={t('login.token')}
        placeholder={t('login.token')}
        invalid={error !== null}
      />
    {/if}
    {#if error}<p class="error" role="alert">{error}</p>{/if}
    <Button type="submit" variant="primary" loading={busy}>
      {registering ? t('login.register') : t('login.submit')}
    </Button>
    {#if canRegister}
      <button type="button" class="link" onclick={toggleMode}>
        {registering ? t('login.haveAccount') : t('login.noAccount')}
      </button>
    {/if}
  </form>
</div>

<style>
  .screen {
    height: 100%;
    display: grid;
    place-items: center;
    background: var(--c-bg);
  }
  .card {
    width: min(360px, 90vw);
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    padding: var(--space-4) calc(var(--space-4) * 1.5);
    border: 1px solid var(--c-border);
    border-radius: var(--radius-lg);
    background: var(--c-bg-elevated);
    box-shadow: 0 20px 60px var(--c-shadow);
  }
  .logo {
    align-self: center;
  }
  h1 {
    text-align: center;
    font-size: 17px;
    font-weight: 600;
  }
  .hint {
    text-align: center;
    color: var(--c-fg-subtle);
  }
  .invite {
    text-align: center;
    padding: var(--space-2);
    border-radius: var(--radius-sm);
    background: var(--c-accent-subtle);
  }
  .error {
    color: var(--c-danger);
    font-size: var(--font-size-sm);
  }
  .link {
    align-self: center;
    border: none;
    background: none;
    color: var(--c-accent);
    cursor: pointer;
    font: inherit;
    font-size: var(--font-size-sm);
  }
  .link:hover {
    text-decoration: underline;
  }
</style>
