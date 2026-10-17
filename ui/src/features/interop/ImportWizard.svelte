<!--
  Import wizard: choose a source (files, a folder, pasted text or a URL),
  preview what will be imported with the report, pick the target and import.
-->
<script lang="ts">
  import type { Format } from '$lib/api/generated/Format';
  import type { ImportApplyOutput } from '$lib/api/generated/ImportApplyOutput';
  import type { ImportBundle } from '$lib/api/generated/ImportBundle';
  import type { ImportSource } from '$lib/api/generated/ImportSource';
  import type { MergeStrategy } from '$lib/api/generated/MergeStrategy';
  import { errorMessage } from '$lib/errors';
  import { t } from '$lib/i18n';
  import {
    chooseImportFiles,
    dropToSources,
    filesToSources,
    saveExportFiles,
  } from '$lib/platform/importFiles';
  import type { Services } from '$lib/services.svelte';
  import Button from '$lib/ui/Button.svelte';
  import Checkbox from '$lib/ui/Checkbox.svelte';
  import Icon from '$lib/ui/Icon.svelte';
  import Input from '$lib/ui/Input.svelte';
  import MethodBadge from '$lib/ui/MethodBadge.svelte';
  import Modal from '$lib/ui/overlay/Modal.svelte';
  import Select from '$lib/ui/overlay/Select.svelte';
  import { toasts } from '$lib/ui/overlay/toasts.svelte';
  import Spinner from '$lib/ui/Spinner.svelte';
  import Tabs from '$lib/ui/Tabs.svelte';
  import Textarea from '$lib/ui/Textarea.svelte';
  import { previewRows, reportText, summarize } from './model';

  interface Props {
    services: Services;
    /** Preselected target collection. */
    collectionId?: string | null;
    onclose: () => void;
  }

  let { services, collectionId = null, onclose }: Props = $props();
  const ws = $derived(services.workspace);

  type Step = 'source' | 'preview' | 'done';
  type SourceKind = 'file' | 'text' | 'url';
  let step = $state<Step>('source');
  let sourceKind = $state<SourceKind>('file');
  let sources = $state<ImportSource[]>([]);
  let text = $state('');
  let url = $state('');
  let busy = $state(false);
  let error = $state<string | null>(null);
  let dragging = $state(false);
  let bundle = $state<ImportBundle | null>(null);
  let format = $state<Format | null>(null);
  // svelte-ignore state_referenced_locally
  let target = $state<string>(collectionId ?? 'new');
  let strategy = $state<MergeStrategy>('merge');
  let withEnvironments = $state(true);
  let withCookies = $state(true);
  let result = $state<ImportApplyOutput | null>(null);

  const FORMATS: { value: Format; label: string }[] = [
    { value: 'reqly', label: 'Reqly' },
    { value: 'insomnia', label: 'Insomnia' },
    { value: 'postman', label: 'Postman' },
    { value: 'openApi', label: 'OpenAPI / Swagger' },
    { value: 'har', label: 'HAR' },
    { value: 'curl', label: 'cURL' },
    { value: 'bruno', label: 'Bruno' },
  ];

  const summary = $derived(bundle ? summarize(bundle) : null);
  const preview = $derived(bundle ? previewRows(bundle) : null);
  const entries = $derived(bundle?.report.entries ?? []);

  function currentSources(): ImportSource[] {
    if (sourceKind === 'text') return text.trim() ? [{ name: 'clipboard', content: text }] : [];
    return sources;
  }

  async function runPreview(override: Format | null = null) {
    const input = currentSources();
    if (input.length === 0) return;
    busy = true;
    error = null;
    try {
      bundle = await services.transport.call('import.preview', {
        sources: input,
        format: override ?? undefined,
      });
      format = bundle.format;
      step = 'preview';
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }

  async function addFiles(files: File[]) {
    if (files.length === 0) return;
    try {
      sources = await filesToSources(files);
      await runPreview();
    } catch (e) {
      error = errorMessage(e);
    }
  }

  async function onDrop(e: DragEvent) {
    e.preventDefault();
    dragging = false;
    if (!e.dataTransfer) return;
    try {
      sources = await dropToSources(e.dataTransfer);
      await runPreview();
    } catch (err) {
      error = errorMessage(err);
    }
  }

  async function loadUrl() {
    if (!url.trim()) return;
    busy = true;
    error = null;
    try {
      const file = await services.transport.call('import.fromUrl', { url: url.trim() });
      sources = [file];
    } catch (e) {
      error = errorMessage(e);
      busy = false;
      return;
    }
    busy = false;
    await runPreview();
  }

  async function apply() {
    const workspaceId = ws.activeWorkspaceId;
    if (!workspaceId || !bundle) return;
    busy = true;
    error = null;
    try {
      result = await services.transport.call('import.apply', {
        sources: currentSources(),
        format: format ?? undefined,
        workspaceId,
        target:
          target === 'new'
            ? { kind: 'newCollections' }
            : { kind: 'collection', collectionId: target, strategy },
        environments: withEnvironments,
        cookies: withCookies,
      });
      await ws.load();
      const first = result.collectionIds[0];
      if (first) await ws.selectCollection(first);
      step = 'done';
      toasts.show({ message: t('import.done', { count: result.requests }), kind: 'success' });
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }

  const report = () =>
    reportText(bundle!.report, t('import.reportTitle', { format: bundle!.format }));

  async function copyReport() {
    await navigator.clipboard.writeText(report());
    toasts.show({ message: t('import.reportCopied'), kind: 'success' });
  }

  async function saveReport() {
    await saveExportFiles(services.transport, [
      { name: 'import-report.txt', mime: 'text/plain', content: report() },
    ]);
  }

  const kindLabel = (kind: string) =>
    // i18n-dynamic: import.kind.
    t(`import.kind.${kind}` as 'import.kind.info');
</script>

<Modal title={t('import.title')} width="760px" {onclose}>
  {#if step === 'source'}
    <Tabs
      label={t('import.source')}
      idPrefix="import"
      bind:value={sourceKind}
      tabs={[
        { id: 'file', label: t('import.fromFile') },
        { id: 'text', label: t('import.fromText') },
        { id: 'url', label: t('import.fromUrl') },
      ]}
    />
    <div class="source" role="tabpanel" id="import-panel-{sourceKind}">
      {#if sourceKind === 'file'}
        <div
          class="drop"
          class:dragging
          role="region"
          aria-label={t('import.dropZone')}
          ondragover={(e) => {
            e.preventDefault();
            dragging = true;
          }}
          ondragleave={() => (dragging = false)}
          ondrop={onDrop}
        >
          <Icon name="download" size={28} />
          <p>{t('import.dropHint')}</p>
          <div class="buttons">
            <Button onclick={async () => addFiles(await chooseImportFiles(false))}>
              {t('import.chooseFiles')}
            </Button>
            <Button onclick={async () => addFiles(await chooseImportFiles(true))}>
              {t('import.chooseFolder')}
            </Button>
          </div>
          <p class="formats">{t('import.formats')}</p>
        </div>
      {:else if sourceKind === 'text'}
        <Textarea
          mono
          bind:value={text}
          rows={12}
          aria-label={t('import.pasteLabel')}
          placeholder={t('import.pastePlaceholder')}
        />
      {:else}
        <div class="url-row">
          <Input
            mono
            bind:value={url}
            aria-label={t('import.urlLabel')}
            placeholder={t('import.urlPlaceholder')}
            onkeydown={(e) => e.key === 'Enter' && void loadUrl()}
          />
          <Button onclick={() => void loadUrl()} disabled={busy}>{t('import.load')}</Button>
        </div>
      {/if}
    </div>
  {:else if step === 'preview' && bundle && summary && preview}
    <div class="preview">
      <div class="row">
        <span class="label">{t('import.format')}</span>
        <div class="format">
          <Select
            label={t('import.format')}
            value={format ?? bundle.format}
            options={FORMATS}
            onchange={(f) => void runPreview(f)}
          />
        </div>
        <span class="counts">
          {t('import.summary', {
            requests: summary.requests,
            folders: summary.folders,
            environments: summary.environments,
          })}
        </span>
      </div>
      <ul class="tree" aria-label={t('import.previewTree')}>
        {#each preview.rows as row, i (i)}
          <li style:padding-left="{row.depth * 16 + 4}px" class={row.kind}>
            {#if row.kind === 'request'}
              <MethodBadge method={row.method ?? 'GET'} />
            {:else}
              <Icon name={row.kind === 'folder' ? 'folder' : 'layers'} size={14} />
            {/if}
            <span>{row.name}</span>
          </li>
        {/each}
        {#if preview.truncated}
          <li class="more">{t('import.truncated')}</li>
        {/if}
      </ul>

      <section class="report" aria-label={t('import.report')}>
        <header>
          <h3>{t('import.report')}</h3>
          {#if entries.length > 0}
            <Button size="sm" onclick={() => void copyReport()}>{t('import.copyReport')}</Button>
            <Button size="sm" onclick={() => void saveReport()}>{t('import.saveReport')}</Button>
          {/if}
        </header>
        {#if entries.length === 0}
          <p class="ok">{t('import.reportEmpty')}</p>
        {:else}
          <ul>
            {#each entries as e, i (i)}
              <li>
                <span class="kind {e.kind}">{kindLabel(e.kind)}</span>
                <span class="path">{e.path}</span>
                <span class="message">{e.message}</span>
              </li>
            {/each}
          </ul>
        {/if}
      </section>

      <div class="target">
        <span class="label">{t('import.target')}</span>
        <Select
          label={t('import.target')}
          bind:value={target}
          options={[
            { value: 'new', label: t('import.newCollection') },
            ...ws.collections.map((c) => ({ value: c.id, label: c.name })),
          ]}
        />
        {#if target !== 'new'}
          <Select
            label={t('import.strategy')}
            bind:value={strategy}
            options={[
              {
                value: 'merge',
                label: t('import.strategyMerge'),
                hint: t('import.strategyMergeHint'),
              },
              {
                value: 'append',
                label: t('import.strategyAppend'),
                hint: t('import.strategyAppendHint'),
              },
              {
                value: 'replace',
                label: t('import.strategyReplace'),
                hint: t('import.strategyReplaceHint'),
              },
            ]}
          />
        {/if}
      </div>
      <div class="options">
        <Checkbox bind:checked={withEnvironments} showLabel label={t('import.withEnvironments')} />
        {#if summary.cookies > 0}
          <Checkbox
            bind:checked={withCookies}
            showLabel
            label={t('import.withCookies', { count: summary.cookies })}
          />
        {/if}
      </div>
    </div>
  {:else if step === 'done' && result}
    <div class="done">
      <Icon name="layers" size={28} />
      <p>{t('import.done', { count: result.requests })}</p>
      {#if result.report.entries.length > 0}
        <p class="hint">{t('import.doneWithReport', { count: result.report.entries.length })}</p>
        <div class="buttons">
          <Button size="sm" onclick={() => void copyReport()}>{t('import.copyReport')}</Button>
          <Button size="sm" onclick={() => void saveReport()}>{t('import.saveReport')}</Button>
        </div>
      {/if}
    </div>
  {/if}

  {#if error}
    <p class="error" role="alert">{error}</p>
  {/if}

  {#snippet footer()}
    {#if busy}<Spinner />{/if}
    {#if step === 'source'}
      <Button onclick={onclose}>{t('common.cancel')}</Button>
      {#if sourceKind === 'text'}
        <Button variant="primary" disabled={busy || !text.trim()} onclick={() => void runPreview()}>
          {t('import.continue')}
        </Button>
      {/if}
    {:else if step === 'preview'}
      <Button
        onclick={() => {
          step = 'source';
          bundle = null;
        }}>{t('import.back')}</Button
      >
      <Button
        variant="primary"
        disabled={busy || (!summary?.requests && !summary?.environments)}
        onclick={() => void apply()}
      >
        {t('import.import')}
      </Button>
    {:else}
      <Button variant="primary" onclick={onclose}>{t('common.close')}</Button>
    {/if}
  {/snippet}
</Modal>

<style>
  .source {
    padding-top: var(--space-3);
    min-height: 240px;
  }
  .drop {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: var(--space-2);
    min-height: 220px;
    border: 2px dashed var(--c-border-strong);
    border-radius: var(--radius-md, 8px);
    color: var(--c-fg-muted);
    text-align: center;
    padding: var(--space-4);
  }
  .drop.dragging {
    border-color: var(--c-accent);
    background: var(--c-accent-subtle);
  }
  .drop p {
    margin: 0;
  }
  .formats {
    font-size: var(--font-size-xs);
    color: var(--c-fg-subtle);
  }
  .buttons {
    display: flex;
    gap: var(--space-2);
  }
  .url-row {
    display: flex;
    gap: var(--space-2);
  }
  .preview {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }
  .row,
  .target {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .label {
    width: 90px;
    flex: none;
    color: var(--c-fg-muted);
  }
  .format {
    width: 200px;
  }
  .counts {
    color: var(--c-fg-muted);
    font-size: var(--font-size-sm);
  }
  .target :global(.select) {
    min-width: 200px;
  }
  .tree {
    list-style: none;
    margin: 0;
    padding: var(--space-1);
    max-height: 220px;
    overflow: auto;
    border: 1px solid var(--c-border);
    border-radius: var(--radius-sm);
    background: var(--c-bg-sunken);
    font-size: var(--font-size-sm);
  }
  .tree li {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    height: 24px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .tree .collection {
    font-weight: 600;
  }
  .tree .more {
    color: var(--c-fg-subtle);
  }
  .report header {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .report h3 {
    margin: 0;
    flex: 1;
    font-size: var(--font-size);
  }
  .report ul {
    list-style: none;
    margin: var(--space-2) 0 0;
    padding: 0;
    max-height: 160px;
    overflow: auto;
    font-size: var(--font-size-sm);
  }
  .report li {
    display: grid;
    grid-template-columns: 100px minmax(120px, 1fr) 2fr;
    gap: var(--space-2);
    padding: 2px 0;
    border-bottom: 1px solid var(--c-border);
  }
  .kind {
    font-weight: 600;
  }
  .kind.unsupported {
    color: var(--c-warning);
  }
  .kind.converted {
    color: var(--c-info);
  }
  .kind.info {
    color: var(--c-fg-muted);
  }
  .path {
    color: var(--c-fg-muted);
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .ok {
    margin: var(--space-1) 0 0;
    color: var(--c-success);
  }
  .options {
    display: flex;
    gap: var(--space-4);
  }
  .done {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-4);
  }
  .done p {
    margin: 0;
  }
  .hint {
    color: var(--c-fg-muted);
  }
  .error {
    margin: var(--space-2) 0 0;
    color: var(--c-danger);
  }
</style>
