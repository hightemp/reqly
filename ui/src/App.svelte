<script lang="ts">
  import { createTransport, isMobile, isTauri } from '$lib/api/transport';
  import { errorMessage } from '$lib/errors';
  import { t } from '$lib/i18n';
  import { tauriWindow } from '$lib/platform/window';
  import { createServices } from '$lib/services.svelte';
  import AppShell from '$features/shell/AppShell.svelte';

  const boot = (async () => {
    const services = await createServices(await createTransport());
    const windowController = isTauri() && !isMobile() ? await tauriWindow() : undefined;
    return { services, windowController };
  })();
</script>

{#await boot}
  <div class="splash" role="status">{t('app.loading')}</div>
{:then { services, windowController }}
  <AppShell {services} {windowController} />
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
