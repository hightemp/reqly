<script lang="ts">
  import GraphqlBody from '../../graphql/GraphqlBody.svelte';
  import { onMount, type Snippet } from 'svelte';
  import type { Body } from '$lib/api/generated/Body';
  import type { KeyValue } from '$lib/api/generated/KeyValue';
  import type { BodyKind } from '$lib/api/generated/BodyKind';
  import { i18n, t } from '$lib/i18n';
  import { pickFile } from '$lib/platform/files';
  import type { Services } from '$lib/services.svelte';
  import Button from '$lib/ui/Button.svelte';
  import CodeEditor, { type CodeLanguage } from '$lib/ui/code/CodeEditor.svelte';
  import EmptyState from '$lib/ui/EmptyState.svelte';
  import KeyValueEditor from '$lib/ui/kv/KeyValueEditor.svelte';
  import Select from '$lib/ui/overlay/Select.svelte';
  import { toasts } from '$lib/ui/overlay/toasts.svelte';

  interface Props {
    services: Services;
    body: Body;
    /** Request whose variables apply to templates. */
    nodeId?: string | null;
    /** Template-aware value cell for form fields. */
    valueCell?: Snippet<[KeyValue, (value: string) => void]>;
    onchange: (body: Body) => void;
  }

  let { services, body, nodeId = null, valueCell, onchange }: Props = $props();
  let editor: CodeEditor | undefined = $state();

  const LANG: Partial<Record<BodyKind, CodeLanguage>> = {
    json: 'json',
    xml: 'xml',
    yaml: 'yaml',
    html: 'html',
    text: 'text',
  };
  const kinds = $derived<{ value: BodyKind; label: string }[]>([
    { value: 'none', label: t('body.none') },
    { value: 'json', label: 'JSON' },
    { value: 'xml', label: 'XML' },
    { value: 'yaml', label: 'YAML' },
    { value: 'text', label: t('body.text') },
    { value: 'html', label: 'HTML' },
    { value: 'formUrlencoded', label: t('body.formUrlencoded') },
    { value: 'multipart', label: t('body.multipart') },
    { value: 'binary', label: t('body.binary') },
    { value: 'graphql', label: 'GraphQL' },
  ]);
  const language = $derived(LANG[body.kind]);

  async function chooseBinary() {
    const picked = await pickFile(services.transport);
    if (picked) onchange({ ...body, file: picked.ref });
  }

  function format() {
    if (!editor?.format())
      toasts.show({ message: t('body.cannotFormat'), kind: 'warning', timeoutMs: 2500 });
  }

  onMount(() => services.hotkeys.on('body.format', format));
</script>

<div class="body">
  <div class="bar">
    <Select
      label={t('body.kind')}
      value={body.kind}
      options={kinds}
      onchange={(kind) => onchange({ ...body, kind })}
    />
    {#if body.kind === 'json' || body.kind === 'xml' || body.kind === 'html'}
      <Button size="sm" variant="ghost" onclick={format}>{t('body.format')}</Button>
    {/if}
  </div>

  <div class="content">
    {#if body.kind === 'none'}
      <EmptyState title={t('body.noneHint')} />
    {:else if body.kind === 'graphql' && nodeId}
      <GraphqlBody
        {services}
        {nodeId}
        value={body.graphql}
        onchange={(graphql) => onchange({ ...body, graphql })}
      />
    {:else if language}
      <div class="code">
        <CodeEditor
          bind:this={editor}
          templates
          {nodeId}
          label={t('body.label')}
          {language}
          value={body.text}
          onchange={(text) => onchange({ ...body, text })}
        />
      </div>
    {:else if body.kind === 'formUrlencoded' || body.kind === 'multipart'}
      <KeyValueEditor
        label={t('body.fields')}
        rows={body.fields}
        separator="="
        {valueCell}
        allowFiles={body.kind === 'multipart'}
        pickFile={async () => {
          const picked = await pickFile(services.transport);
          return picked ? { value: picked.value, name: picked.name } : null;
        }}
        onchange={(fields) => onchange({ ...body, fields })}
      />
    {:else if body.kind === 'binary'}
      <div class="binary">
        {#if body.file}
          <span class="file"
            >{body.file.type === 'path'
              ? body.file.path
              : `${body.file.name} · ${i18n.formatBytes(body.file.size)}`}</span
          >
        {:else}
          <span class="hint">{t('body.noFile')}</span>
        {/if}
        <Button size="sm" onclick={chooseBinary}>{t('kv.chooseFile')}</Button>
        {#if body.file}<Button
            size="sm"
            variant="ghost"
            onclick={() => onchange({ ...body, file: null })}>{t('body.clearFile')}</Button
          >{/if}
      </div>
    {/if}
  </div>
</div>

<style>
  .body {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    height: 100%;
  }
  .bar {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .content {
    flex: 1;
    min-height: 0;
  }
  .code {
    height: 100%;
    border: 1px solid var(--c-border);
    border-radius: var(--radius-sm);
    background: var(--c-bg-sunken);
    overflow: hidden;
  }
  .binary {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .file {
    font-family: var(--font-mono);
    font-size: var(--font-size-sm);
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .hint {
    color: var(--c-fg-subtle);
  }
</style>
