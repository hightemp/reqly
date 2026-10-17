<!-- Folder settings: headers, auth, network settings and docs inherited by everything inside. -->
<script lang="ts">
  import { onMount } from 'svelte';
  import type { KeyValue } from '$lib/api/generated/KeyValue';
  import { t } from '$lib/i18n';
  import type { Services } from '$lib/services.svelte';
  import type { Draft } from '$lib/stores/editor.svelte';
  import Button from '$lib/ui/Button.svelte';
  import TemplateInput from '$lib/ui/code/TemplateInput.svelte';
  import KeyValueEditor from '$lib/ui/kv/KeyValueEditor.svelte';
  import Modal from '$lib/ui/overlay/Modal.svelte';
  import Spinner from '$lib/ui/Spinner.svelte';
  import Tabs from '$lib/ui/Tabs.svelte';
  import VariablesEditor from '../environments/VariablesEditor.svelte';
  import AuthPanel from '../request/panels/AuthPanel.svelte';
  import DocsPanel from '../request/panels/DocsPanel.svelte';
  import SettingsPanel from '../request/panels/SettingsPanel.svelte';
  import { COMMON_HEADERS } from '../request/headers';

  interface Props {
    services: Services;
    folderId: string;
    onclose: () => void;
  }

  let { services, folderId, onclose }: Props = $props();
  let draft = $state<Draft | null>(null);
  let section = $state<'headers' | 'auth' | 'variables' | 'settings' | 'docs'>('headers');
  const folder = $derived(draft?.folder ?? null);

  onMount(() => {
    void services.editor.open(folderId).then((d) => (draft = d));
  });

  function edit(mutate: Parameters<typeof services.editor.editFolder>[1]) {
    services.editor.editFolder(folderId, mutate);
  }

  async function close() {
    await services.editor.flush(folderId);
    onclose();
  }
</script>

{#snippet valueCell(row: KeyValue, set: (value: string) => void)}
  <TemplateInput value={row.value} label={t('kv.value')} nodeId={folderId} onchange={set} />
{/snippet}

<Modal
  title={t('folder.title', { name: draft?.node?.name ?? '' })}
  width="760px"
  onclose={() => void close()}
>
  {#if !folder}
    <div class="loading"><Spinner /></div>
  {:else}
    <p class="hint">{t('folder.hint')}</p>
    <Tabs
      label={t('folder.sections')}
      idPrefix="folder"
      bind:value={section}
      tabs={[
        {
          id: 'headers',
          label: t('request.headers'),
          count: folder.headers.filter((h) => h.enabled && h.name).length,
        },
        { id: 'auth', label: t('request.auth'), dot: folder.auth.type !== 'inherit' },
        {
          id: 'variables',
          label: t('request.variables'),
          count: Object.keys(folder.variables).length,
        },
        { id: 'settings', label: t('request.settings') },
        { id: 'docs', label: t('request.docs') },
      ]}
    />
    <div class="panel">
      {#if section === 'headers'}
        <KeyValueEditor
          label={t('request.headers')}
          rows={folder.headers}
          nameSuggestions={COMMON_HEADERS}
          {valueCell}
          onchange={(headers) => edit((f) => ({ ...f, headers }))}
        />
      {:else if section === 'auth'}
        <AuthPanel
          auth={folder.auth}
          nodeId={folderId}
          transport={services.transport}
          onchange={(auth) => edit((f) => ({ ...f, auth }))}
        />
      {:else if section === 'variables'}
        <VariablesEditor
          label={t('request.variables')}
          data={folder.variables}
          nodeId={folderId}
          onchange={(variables) => edit((f) => ({ ...f, variables }))}
        />
      {:else if section === 'settings'}
        <SettingsPanel
          settings={folder.settings}
          onchange={(settings) => edit((f) => ({ ...f, settings }))}
        />
      {:else}
        <DocsPanel
          text={folder.description}
          onchange={(description) => edit((f) => ({ ...f, description }))}
        />
      {/if}
    </div>
  {/if}
  {#snippet footer()}
    <Button variant="primary" onclick={() => void close()}>{t('common.close')}</Button>
  {/snippet}
</Modal>

<style>
  .loading {
    display: grid;
    place-items: center;
    min-height: 200px;
  }
  .hint {
    color: var(--c-fg-subtle);
    margin-bottom: var(--space-2);
  }
  .panel {
    padding: var(--space-3) 0;
    min-height: 320px;
    max-height: 60vh;
    overflow: auto;
  }
</style>
