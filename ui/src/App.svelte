<script lang="ts">
  import { createTransport, isMobile, isTauri } from '$lib/api/transport';
  import { errorMessage } from '$lib/errors';
  import { t } from '$lib/i18n';
  import { tauriWindow } from '$lib/platform/window';
  import { createServices } from '$lib/services.svelte';
  import AppShell from '$features/shell/AppShell.svelte';

  import { TRANSPORT_TOPICS } from '$lib/api/transport';
  import LoginScreen from '$features/auth/LoginScreen.svelte';

  type Boot =
    | { kind: 'login' }
    | {
        kind: 'app';
        services: Awaited<ReturnType<typeof createServices>>;
        windowController?: Awaited<ReturnType<typeof tauriWindow>>;
      };

  async function start(): Promise<Boot> {
    if (!isTauri()) {
      const status = await fetch('/api/v1/auth/status', { credentials: 'same-origin' })
        .then((r) =>
          r.ok ? (r.json() as Promise<{ mode: string; authenticated: boolean }>) : null,
        )
        .catch(() => null);
      if (status && !status.authenticated) return { kind: 'login' };
    }
    const transport = await createTransport();
    transport.subscribe((e) => {
      if (e.topic === TRANSPORT_TOPICS.unauthorized) boot = Promise.resolve({ kind: 'login' });
    });
    const services = await createServices(transport);
    const windowController = isTauri() && !isMobile() ? await tauriWindow() : undefined;
    return { kind: 'app', services, windowController };
  }

  let boot = $state<Promise<Boot>>(start());
</script>

{#await boot}
  <div class="splash" role="status">{t('app.loading')}</div>
{:then state}
  {#if state.kind === 'login'}
    <LoginScreen onsuccess={() => location.reload()} />
  {:else}
    <AppShell services={state.services} windowController={state.windowController} />
  {/if}
{:catch error}
  <div class="splash" role="alert">{errorMessage(error)}</div>
{/await}

<style>
  .splash {
    height: 100%;
    display: grid;
    place-items: center;
    color: var(--c-fg-subtle);
  }
</style>
