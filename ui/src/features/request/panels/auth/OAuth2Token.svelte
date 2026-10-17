<!-- Token of an OAuth 2.0 request or folder: status, sign-in and clearing. -->
<script lang="ts">
  import type { OAuth2Config } from '$lib/api/generated/OAuth2Config';
  import type { OAuth2DeviceOutput } from '$lib/api/generated/OAuth2DeviceOutput';
  import type { OAuth2TokenInfo } from '$lib/api/generated/OAuth2TokenInfo';
  import type { Transport } from '$lib/api/transport';
  import { errorMessage } from '$lib/errors';
  import { t } from '$lib/i18n';
  import { signIn, SignInError, webRedirectUri } from '$lib/platform/oauth2';
  import Button from '$lib/ui/Button.svelte';
  import { toasts } from '$lib/ui/overlay/toasts.svelte';

  interface Props {
    transport: Transport;
    nodeId: string;
    config: OAuth2Config;
  }

  let { transport, nodeId, config }: Props = $props();
  let info = $state<OAuth2TokenInfo | null>(null);
  let busy = $state(false);
  let device = $state<OAuth2DeviceOutput | null>(null);
  let cancelled = false;

  const interactive = $derived(
    config.grant === 'authorizationCode' ||
      config.grant === 'implicit' ||
      config.grant === 'deviceCode',
  );
  const redirectHint = $derived(config.redirectUrl.trim() ? null : webRedirectUri());

  async function load() {
    try {
      info = await transport.call('oauth2.status', { nodeId });
    } catch {
      info = null;
    }
  }

  // Settings are saved shortly after editing; re-read the status after that.
  $effect(() => {
    void nodeId;
    void JSON.stringify(config);
    const timer = setTimeout(load, 600);
    return () => clearTimeout(timer);
  });

  async function run(action: () => Promise<void>) {
    busy = true;
    try {
      await action();
    } catch (e) {
      if (e instanceof SignInError) {
        if (e.code === 'popupBlocked') toasts.error(t('auth.token2.popupBlocked'));
      } else {
        toasts.error(errorMessage(e));
      }
    } finally {
      busy = false;
      device = null;
    }
  }

  const getToken = () =>
    run(async () => {
      if (config.grant === 'deviceCode') return deviceFlow();
      if (interactive) {
        const flow = await transport.call('oauth2.authorize', {
          nodeId,
          defaultRedirectUri: webRedirectUri(),
        });
        const callbackUrl = await signIn(
          flow.authorizeUrl,
          flow.redirectUri,
          t('auth.token2.signIn'),
        );
        info = await transport.call('oauth2.complete', { flowId: flow.flowId, callbackUrl });
        return;
      }
      info = await transport.call('oauth2.fetch', { nodeId });
    });

  async function deviceFlow() {
    cancelled = false;
    const start = await transport.call('oauth2.deviceStart', { nodeId });
    device = start;
    let interval = Math.max(1, start.interval) * 1000;
    const deadline = Date.now() + start.expiresIn * 1000;
    while (!cancelled && Date.now() < deadline) {
      await new Promise((r) => setTimeout(r, interval));
      if (cancelled) return;
      const poll = await transport.call('oauth2.devicePoll', { flowId: start.flowId });
      if (poll.status === 'done') {
        info = poll.token ?? null;
        return;
      }
      if (poll.status === 'slowDown') interval += 5000;
    }
  }

  const refresh = () =>
    run(async () => {
      info = await transport.call('oauth2.fetch', { nodeId });
    });

  const clear = () =>
    run(async () => {
      await transport.call('oauth2.clear', { nodeId });
      await load();
    });

  const time = (ms: number | null) => (ms == null ? '' : new Date(ms).toLocaleString());
  const statusText = $derived(
    !info || info.status === 'none'
      ? t('auth.token2.none')
      : info.status === 'valid'
        ? t('auth.token2.valid')
        : info.status === 'expired'
          ? t('auth.token2.expired')
          : t('auth.token2.stale'),
  );
</script>

<section class="token" aria-label={t('auth.token2.title')}>
  <header>
    <h3>{t('auth.token2.title')}</h3>
    <span class="status" data-status={info?.status ?? 'none'}>{statusText}</span>
  </header>

  {#if info && info.status !== 'none'}
    <code class="value" title={info.accessToken}>{info.accessToken}</code>
    <p class="meta">
      {info.expiresAt != null
        ? t('auth.token2.expires', { time: time(info.expiresAt) })
        : t('auth.token2.noExpiry')}
      {#if info.obtainedAt != null}
        · {t('auth.token2.obtained', { time: time(info.obtainedAt) })}
      {/if}
      {#if info.scope}· {info.scope}{/if}
    </p>
  {/if}

  {#if device}
    <p class="device">
      {t('auth.token2.deviceCode', { url: device.verificationUri })}
      <strong class="code">{device.userCode}</strong>
    </p>
    <p class="meta">{t('auth.token2.waiting')}</p>
  {/if}

  <div class="actions">
    {#if device}
      <Button size="sm" onclick={() => (cancelled = true)}>{t('auth.token2.cancel')}</Button>
    {:else}
      <Button size="sm" variant="primary" loading={busy} onclick={getToken}>
        {interactive ? t('auth.token2.signIn') : t('auth.token2.get')}
      </Button>
      {#if info?.hasRefreshToken}
        <Button size="sm" disabled={busy} onclick={refresh}>{t('auth.token2.refresh')}</Button>
      {/if}
      {#if info && info.status !== 'none'}
        <Button
          size="sm"
          variant="ghost"
          onclick={() => navigator.clipboard?.writeText(info?.accessToken ?? '')}
          >{t('auth.token2.copy')}</Button
        >
        <Button size="sm" variant="ghost" disabled={busy} onclick={clear}
          >{t('auth.token2.clear')}</Button
        >
      {/if}
    {/if}
  </div>
  {#if interactive && config.grant !== 'deviceCode' && redirectHint}
    <p class="meta">{t('auth.token2.redirectHint', { url: redirectHint })}</p>
  {/if}
</section>

<style>
  .token {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding: var(--space-3);
    border: 1px solid var(--c-border);
    border-radius: var(--radius-md);
    background: var(--c-bg-sunken);
  }
  header {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  h3 {
    font-size: var(--font-size-sm);
    font-weight: 600;
  }
  .status {
    color: var(--c-fg-muted);
    font-size: var(--font-size-xs);
  }
  .status[data-status='valid'] {
    color: var(--c-success, var(--c-fg-muted));
  }
  .status[data-status='expired'],
  .status[data-status='stale'] {
    color: var(--c-warning, var(--c-fg-muted));
  }
  .value {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: var(--font-mono);
    font-size: var(--font-size-xs);
  }
  .meta {
    color: var(--c-fg-subtle);
    font-size: var(--font-size-xs);
  }
  .code {
    margin-left: var(--space-2);
    font-family: var(--font-mono);
    letter-spacing: 0.1em;
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
  }
</style>
