<script lang="ts">
  import { onMount } from 'svelte';
  import type { AppInfoOutput } from '$lib/api/generated/AppInfoOutput';
  import { t } from '$lib/i18n';
  import type { Services } from '$lib/services.svelte';
  import Button from '$lib/ui/Button.svelte';
  import { toasts } from '$lib/ui/overlay/toasts.svelte';
  import Field from '../Field.svelte';

  let { services }: { services: Services } = $props();
  let info = $state<AppInfoOutput | null>(null);
  let cleaning = $state(false);

  onMount(async () => {
    info = await services.transport.call('app.info', {});
  });

  async function cleanup() {
    cleaning = true;
    try {
      const out = await services.transport.call('maintenance.cleanup', {});
      toasts.show({
        message: t('settings.cleanupDone', { count: out.removedBlobs }),
        kind: 'success',
      });
    } finally {
      cleaning = false;
    }
  }
</script>

{#if info}
  <Field label={t('settings.version')}>
    <span class="mono">{info.name} {info.version}</span>
  </Field>
  <Field label={t('settings.platform')}>
    <span class="mono">{info.platform}</span>
  </Field>
  {#if info.dataDir}
    <Field label={t('settings.dataDir')}>
      <span class="mono selectable">{info.dataDir}</span>
    </Field>
  {/if}
{/if}
<Field label={t('settings.cleanup')} hint={t('settings.cleanupHint')}>
  <Button size="sm" loading={cleaning} onclick={() => void cleanup()}
    >{t('settings.cleanupRun')}</Button
  >
</Field>

<style>
  .mono {
    font-family: var(--font-mono);
    font-size: var(--font-size-sm);
    word-break: break-all;
  }
</style>
