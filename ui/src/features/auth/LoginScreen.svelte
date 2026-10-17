<!-- Token login for the server build. -->
<script lang="ts">
  import { onMount } from 'svelte';
  import { t } from '$lib/i18n';
  import Button from '$lib/ui/Button.svelte';
  import Input from '$lib/ui/Input.svelte';

  let { onsuccess }: { onsuccess: () => void } = $props();
  let token = $state('');
  let error = $state<string | null>(null);
  let busy = $state(false);
  let input: HTMLInputElement | null = $state(null);

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    if (!token.trim()) return;
    busy = true;
    error = null;
    try {
      const res = await fetch('/api/v1/auth/login', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        credentials: 'same-origin',
        body: JSON.stringify({ token: token.trim() }),
      });
      if (res.ok) onsuccess();
      else error = res.status === 429 ? t('login.tooMany') : t('login.invalid');
    } catch {
      error = t('errors.transport.network');
    } finally {
      busy = false;
    }
  }

  onMount(() => input?.focus());
</script>

<div class="screen">
  <form class="card" onsubmit={submit}>
    <img class="logo" src="/favicon.svg" alt="" width="48" height="48" />
    <h1>{t('login.title')}</h1>
    <p class="hint">{t('login.hint')}</p>
    <Input
      bind:inputRef={input}
      bind:value={token}
      type="password"
      aria-label={t('login.token')}
      placeholder={t('login.token')}
      invalid={error !== null}
    />
    {#if error}<p class="error" role="alert">{error}</p>{/if}
    <Button type="submit" variant="primary" loading={busy}>{t('login.submit')}</Button>
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
  .error {
    color: var(--c-danger);
    font-size: var(--font-size-sm);
  }
</style>
