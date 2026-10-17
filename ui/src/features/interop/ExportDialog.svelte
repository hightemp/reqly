<!-- Export a workspace, collection or selected items in one of the supported formats. -->
<script lang="ts" module>
  import type { ExportScope } from '$lib/api/generated/ExportScope';

  export type ExportRequest = { scope: ExportScope; title: string };
</script>

<script lang="ts">
  import type { ExportFormat } from '$lib/api/generated/ExportFormat';
  import { errorMessage } from '$lib/errors';
  import { t } from '$lib/i18n';
  import { saveExportFiles } from '$lib/platform/importFiles';
  import type { Services } from '$lib/services.svelte';
  import Button from '$lib/ui/Button.svelte';
  import Checkbox from '$lib/ui/Checkbox.svelte';
  import Modal from '$lib/ui/overlay/Modal.svelte';
  import Select from '$lib/ui/overlay/Select.svelte';
  import { toasts } from '$lib/ui/overlay/toasts.svelte';

  interface Props {
    services: Services;
    request: ExportRequest;
    onclose: () => void;
  }

  let { services, request, onclose }: Props = $props();
  const FORMAT_KEY = 'ui.exportFormat';

  let format = $state<ExportFormat>('reqlyJson');
  let environments = $state(true);
  let secrets = $state(false);
  let cookies = $state(false);
  let history = $state(false);
  let busy = $state(false);
  let error = $state<string | null>(null);

  $effect(() => {
    void services.transport.call('settings.get', { key: FORMAT_KEY }).then(({ value }) => {
      if (typeof value === 'string') format = value as ExportFormat;
    });
  });

  const reqly = $derived(format === 'reqlyJson' || format === 'reqlyYaml');
  const formats = $derived<{ value: ExportFormat; label: string; hint?: string }[]>([
    { value: 'reqlyJson', label: t('export.reqlyJson') },
    { value: 'reqlyYaml', label: t('export.reqlyYaml') },
    { value: 'insomnia4', label: 'Insomnia v4 (JSON)' },
    {
      value: 'insomnia5',
      label: 'Insomnia v5 (YAML)',
      hint: request.scope.kind === 'workspace' ? t('export.oneCollection') : undefined,
    },
    { value: 'postman', label: 'Postman v2.1', hint: t('export.postmanHint') },
    { value: 'har', label: 'HAR 1.2' },
    { value: 'curl', label: t('export.curl') },
  ]);

  async function run() {
    busy = true;
    error = null;
    try {
      void services.transport.call('settings.set', { key: FORMAT_KEY, value: format });
      const out = await services.transport.call('export.run', {
        scope: request.scope,
        format,
        options: {
          environments,
          secrets: reqly && secrets,
          cookies: reqly && cookies,
          history: reqly && history,
        },
      });
      if (await saveExportFiles(services.transport, out.files)) {
        toasts.show({ message: t('export.saved', { count: out.files.length }), kind: 'success' });
        onclose();
      }
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }
</script>

<Modal title={t('export.title', { name: request.title })} width="520px" {onclose}>
  <div class="form">
    <div class="field">
      <span class="label">{t('export.format')}</span>
      <Select label={t('export.format')} bind:value={format} options={formats} />
    </div>
    <div class="options">
      <Checkbox bind:checked={environments} showLabel label={t('export.environments')} />
      {#if reqly}
        <Checkbox bind:checked={secrets} showLabel label={t('export.secrets')} />
        {#if secrets}
          <p class="warning" role="note">{t('export.secretsWarning')}</p>
        {/if}
        <Checkbox bind:checked={cookies} showLabel label={t('export.cookies')} />
        <Checkbox bind:checked={history} showLabel label={t('export.history')} />
      {:else}
        <p class="hint">{t('export.foreignHint')}</p>
      {/if}
    </div>
    {#if error}
      <p class="error" role="alert">{error}</p>
    {/if}
  </div>
  {#snippet footer()}
    <Button onclick={onclose}>{t('common.cancel')}</Button>
    <Button variant="primary" disabled={busy} onclick={() => void run()}
      >{t('export.export')}</Button
    >
  {/snippet}
</Modal>

<style>
  .form {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }
  .field {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .field :global(.select) {
    flex: 1;
  }
  .label {
    width: 90px;
    color: var(--c-fg-muted);
  }
  .options {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }
  .warning {
    margin: 0 0 0 var(--space-5, 24px);
    color: var(--c-warning);
    font-size: var(--font-size-sm);
  }
  .hint {
    margin: 0;
    color: var(--c-fg-subtle);
    font-size: var(--font-size-sm);
  }
  .error {
    margin: 0;
    color: var(--c-danger);
  }
</style>
