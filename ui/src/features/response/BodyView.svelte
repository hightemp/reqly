<!-- Response body: Pretty / Raw / Preview, filters, large-body handling. -->
<script lang="ts">
  import type { ResponseRecord } from '$lib/api/generated/ResponseRecord';
  import type { FilterKind } from '$lib/api/generated/FilterKind';
  import { errorMessage } from '$lib/errors';
  import { i18n, t } from '$lib/i18n';
  import { readResponseBody, saveResponseBody } from '$lib/platform/files';
  import type { Services } from '$lib/services.svelte';
  import Button from '$lib/ui/Button.svelte';
  import CodeEditor, { type CodeLanguage } from '$lib/ui/code/CodeEditor.svelte';
  import { formatJson, formatXml } from '$lib/ui/code/format';
  import EmptyState from '$lib/ui/EmptyState.svelte';
  import IconButton from '$lib/ui/IconButton.svelte';
  import Select from '$lib/ui/overlay/Select.svelte';
  import Spinner from '$lib/ui/Spinner.svelte';
  import { tooltip } from '$lib/ui/overlay/tooltip.svelte';
  import Tabs from '$lib/ui/Tabs.svelte';
  import { charsetOf, decodeBody, detectView, mimeOf, type BodyView } from './body';

  interface Props {
    services: Services;
    record: ResponseRecord;
  }

  let { services, record }: Props = $props();

  /** Above this, pretty mode is not used automatically. */
  const PRETTY_LIMIT = 5 * 1024 * 1024;
  /** Never load more than this into the UI. */
  const LOAD_LIMIT = 20 * 1024 * 1024;

  type Mode = 'pretty' | 'raw' | 'preview';
  let bytes = $state<Uint8Array | null>(null);
  let loading = $state(false);
  let mode = $state<Mode>('pretty');
  // svelte-ignore state_referenced_locally
  let wrap = $state(services.prefs.value.editorWrap);
  let forceLarge = $state(false);
  let filterKind = $state<FilterKind>('jsonPath');
  let filterText = $state('');
  let filterResult = $state<string | null>(null);
  let filterError = $state<string | null>(null);
  let objectUrl = $state<string | null>(null);

  const large = $derived(record.bodySize > PRETTY_LIMIT);
  const view = $derived<BodyView>(bytes ? detectView(record.contentType, bytes) : 'text');
  const text = $derived(
    bytes && !['image', 'pdf', 'audio', 'video', 'binary'].includes(view)
      ? decodeBody(bytes, charsetOf(record.contentType))
      : '',
  );
  const canPreview = $derived(['html', 'image', 'pdf', 'audio', 'video'].includes(view));
  const canFilter = $derived(view === 'json' || view === 'xml');
  const language = $derived<CodeLanguage>(
    view === 'json' ? 'json' : view === 'xml' ? 'xml' : view === 'html' ? 'html' : 'text',
  );

  const pretty = $derived.by(() => {
    if (mode !== 'pretty') return text;
    if (view === 'json') return formatJson(text) ?? text;
    if (view === 'xml') return formatXml(text);
    return text;
  });

  $effect(() => {
    const id = record.id;
    const limit = large && !forceLarge ? 256 * 1024 : LOAD_LIMIT;
    bytes = null;
    filterResult = null;
    filterError = null;
    if (!record.bodyHash) {
      bytes = new Uint8Array();
      return;
    }
    loading = true;
    void readResponseBody(services.transport, id, { maxBytes: limit })
      .then((b) => {
        if (record.id === id) bytes = b;
      })
      .finally(() => (loading = false));
  });

  // Default mode per content kind.
  $effect(() => {
    if (!bytes) return;
    mode =
      view === 'image' || view === 'pdf' || view === 'audio' || view === 'video'
        ? 'preview'
        : 'pretty';
    filterKind = view === 'xml' ? 'xPath' : 'jsonPath';
  });

  $effect(() => {
    if (!bytes || !canPreview || mode !== 'preview' || view === 'html') {
      objectUrl = null;
      return;
    }
    const url = URL.createObjectURL(
      new Blob([bytes.slice().buffer], { type: mimeOf(record.contentType) || undefined }),
    );
    objectUrl = url;
    return () => URL.revokeObjectURL(url);
  });

  async function runFilter() {
    if (!filterText.trim()) {
      filterResult = null;
      filterError = null;
      return;
    }
    try {
      const out = await services.transport.call('response.filter', {
        id: record.id,
        kind: filterKind,
        expression: filterText,
      });
      filterResult = out.result;
      filterError = null;
    } catch (e) {
      filterError = errorMessage(e);
    }
  }

  function suggestedName() {
    const ext =
      { json: 'json', xml: 'xml', html: 'html', pdf: 'pdf', text: 'txt' }[view as string] ?? 'bin';
    return `response.${ext}`;
  }
</script>

<div class="body">
  <div class="bar">
    <Tabs
      label={t('response.bodyModes')}
      idPrefix="body-mode"
      bind:value={mode}
      tabs={[
        { id: 'pretty', label: t('response.pretty') },
        { id: 'raw', label: t('response.raw') },
        ...(canPreview ? [{ id: 'preview' as const, label: t('response.preview') }] : []),
      ]}
    />
    <span class="spacer"></span>
    <IconButton
      icon="wrap"
      size={14}
      pressed={wrap}
      label={t('response.wrap')}
      onclick={() => (wrap = !wrap)}
    />
    <IconButton
      icon="copy"
      size={14}
      label={t('response.copyBody')}
      onclick={() => void navigator.clipboard.writeText(filterResult ?? text)}
    />
    <IconButton
      icon="download"
      size={14}
      label={t('response.saveBody')}
      onclick={() => void saveResponseBody(services.transport, record.id, suggestedName())}
    />
  </div>

  <div class="content">
    {#if loading && !bytes}
      <div class="center"><Spinner /></div>
    {:else if !bytes || record.bodySize === 0}
      <EmptyState title={t('response.noBody')} />
    {:else if large && !forceLarge && mode !== 'raw'}
      <EmptyState title={t('response.tooLarge', { size: i18n.formatBytes(record.bodySize) })}>
        <div class="actions">
          <Button size="sm" onclick={() => (mode = 'raw')}>{t('response.showRaw')}</Button>
          <Button size="sm" onclick={() => (forceLarge = true)}>{t('response.loadAnyway')}</Button>
          <Button
            size="sm"
            variant="primary"
            onclick={() => void saveResponseBody(services.transport, record.id, suggestedName())}
          >
            {t('response.saveBody')}
          </Button>
        </div>
      </EmptyState>
    {:else if mode === 'preview' && view === 'html'}
      <!-- Scripts disabled: previews never run response code. -->
      <iframe class="preview" title={t('response.preview')} sandbox="" srcdoc={text}></iframe>
    {:else if mode === 'preview' && view === 'image' && objectUrl}
      <div class="media"><img src={objectUrl} alt={t('response.preview')} /></div>
    {:else if mode === 'preview' && view === 'pdf' && objectUrl}
      <iframe class="preview" title={t('response.preview')} src={objectUrl}></iframe>
    {:else if mode === 'preview' && view === 'audio' && objectUrl}
      <div class="media"><audio controls src={objectUrl}></audio></div>
    {:else if mode === 'preview' && view === 'video' && objectUrl}
      <div class="media"><video controls src={objectUrl}><track kind="captions" /></video></div>
    {:else if view === 'binary' && mode !== 'raw'}
      <EmptyState title={t('response.binary', { size: i18n.formatBytes(record.bodySize) })}>
        <Button
          size="sm"
          onclick={() => void saveResponseBody(services.transport, record.id, suggestedName())}
          >{t('response.saveBody')}</Button
        >
      </EmptyState>
    {:else}
      <div class="code">
        <CodeEditor
          label={t('response.body')}
          readonly
          {wrap}
          language={filterResult !== null ? language : mode === 'raw' ? 'text' : language}
          value={filterResult ??
            (mode === 'raw' ? (large && !forceLarge ? `${text}\n…` : text) : pretty)}
        />
      </div>
    {/if}
  </div>

  {#if canFilter && bytes}
    <div class="filter">
      <Select
        compact
        label={t('response.filterKind')}
        value={filterKind}
        options={view === 'xml'
          ? [{ value: 'xPath', label: 'XPath' }]
          : [
              { value: 'jsonPath', label: 'JSONPath' },
              { value: 'jq', label: 'jq' },
            ]}
        onchange={(k) => {
          filterKind = k;
          void runFilter();
        }}
      />
      <input
        class="filter-input"
        class:invalid={filterError !== null}
        bind:value={filterText}
        spellcheck="false"
        aria-label={t('response.filter')}
        placeholder={filterKind === 'jq'
          ? '.items[] | .name'
          : filterKind === 'xPath'
            ? '//item/name'
            : '$.items[*].name'}
        oninput={() => void runFilter()}
      />
      {#if filterError}<span class="filter-error" use:tooltip={filterError}>{filterError}</span
        >{/if}
    </div>
  {/if}
</div>

<style>
  .body {
    height: 100%;
    display: flex;
    flex-direction: column;
    min-height: 0;
  }
  .bar {
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 0 var(--space-2);
    border-bottom: 1px solid var(--c-border);
  }
  .spacer {
    flex: 1;
  }
  .content {
    flex: 1;
    min-height: 0;
    position: relative;
  }
  .center {
    height: 100%;
    display: grid;
    place-items: center;
  }
  .code {
    height: 100%;
  }
  .actions {
    display: flex;
    gap: var(--space-2);
  }
  .preview {
    width: 100%;
    height: 100%;
    border: 0;
    background: #fff;
  }
  .media {
    height: 100%;
    display: grid;
    place-items: center;
    overflow: auto;
    padding: var(--space-3);
    background-image: repeating-conic-gradient(var(--c-bg-hover) 0% 25%, transparent 0% 50%);
    background-size: 16px 16px;
  }
  .media img {
    max-width: 100%;
    max-height: 100%;
  }
  .filter {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-1) var(--space-2);
    border-top: 1px solid var(--c-border);
  }
  .filter-input {
    flex: 1;
    min-width: 0;
    height: 24px;
    padding: 0 var(--space-2);
    border-radius: var(--radius-sm);
    border: 1px solid var(--c-border);
    background: var(--c-bg-sunken);
    color: var(--c-fg);
    font-family: var(--font-mono);
    font-size: var(--font-size-sm);
    outline: none;
  }
  .filter-input:focus {
    border-color: var(--c-accent);
  }
  .filter-input.invalid {
    border-color: var(--c-danger);
  }
  .filter-error {
    max-width: 40%;
    color: var(--c-danger);
    font-size: var(--font-size-xs);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
