<!-- Editor for the active tab's request: URL bar and Params/Body/Auth/Headers/Settings/Docs. -->
<script lang="ts">
  import { onMount } from 'svelte';
  import type { HttpRequest } from '$lib/api/generated/HttpRequest';
  import type { KeyValue } from '$lib/api/generated/KeyValue';
  import { errorMessage } from '$lib/errors';
  import { t } from '$lib/i18n';
  import type { Services } from '$lib/services.svelte';
  import type { Draft } from '$lib/stores/editor.svelte';
  import TemplateInput from '$lib/ui/code/TemplateInput.svelte';
  import EmptyState from '$lib/ui/EmptyState.svelte';
  import KeyValueEditor, { type InheritedRow } from '$lib/ui/kv/KeyValueEditor.svelte';
  import { dialogs } from '$lib/ui/overlay/dialogs.svelte';
  import { toasts } from '$lib/ui/overlay/toasts.svelte';
  import Spinner from '$lib/ui/Spinner.svelte';
  import Tabs from '$lib/ui/Tabs.svelte';
  import VariablesEditor from '../environments/VariablesEditor.svelte';
  import { interopDialogs } from '../interop/dialogs.svelte';
  import { COMMON_HEADERS } from './headers';
  import AuthPanel from './panels/AuthPanel.svelte';
  import BodyPanel from './panels/BodyPanel.svelte';
  import DocsPanel from './panels/DocsPanel.svelte';
  import SettingsPanel from './panels/SettingsPanel.svelte';
  import { streamKind } from '$lib/protocols';
  import ScriptsPanel from '../scripts/ScriptsPanel.svelte';
  import GrpcPanel from '../stream/GrpcPanel.svelte';
  import WebSocketPanel from '../stream/WebSocketPanel.svelte';
  import UrlBar from './UrlBar.svelte';
  import { syncPathParams } from './urlsync';

  let { services }: { services: Services } = $props();

  type Section =
    'params' | 'body' | 'auth' | 'headers' | 'scripts' | 'variables' | 'settings' | 'docs';
  // eslint-disable-next-line svelte/prefer-svelte-reactivity -- remembered tab per request, not rendered state
  const sections = new Map<string, Section>();

  let draft = $state<Draft | null>(null);
  let section = $state<Section>('params');
  let urlBar: UrlBar | undefined = $state();
  let folderHeaders = $state<InheritedRow[]>([]);

  const activeId = $derived(services.tabs.activeId);
  const req = $derived(draft?.request ?? null);
  const run = $derived(activeId ? services.responses.run(activeId) : null);
  /** SSE requests and GraphQL subscriptions connect instead of sending. */
  const streaming = $derived(req ? streamKind(draft?.node?.data) !== null : false);
  const streamRun = $derived(activeId && streaming ? services.streams.run(activeId) : null);

  $effect(() => {
    const id = activeId;
    draft = null;
    if (!id) return;
    void services.editor.open(id).then(
      (d) => {
        if (services.tabs.activeId !== id) return;
        draft = d;
        section = sections.get(id) ?? (d.request?.body.kind !== 'none' ? 'body' : 'params');
      },
      () => services.tabs.close(id),
    );
  });

  // Headers inherited from ancestor folders (nearest wins), shown read-only.
  $effect(() => {
    const id = activeId;
    const chain: string[] = [];
    let p = id ? services.tree.parentOf(id) : null;
    while (p) {
      chain.unshift(p);
      p = services.tree.parentOf(p);
    }
    void Promise.all(chain.map((f) => services.editor.open(f))).then((folders) => {
      // eslint-disable-next-line svelte/prefer-svelte-reactivity -- local accumulator
      const rows = new Map<string, InheritedRow>();
      for (const f of folders) {
        for (const h of f.folder?.headers ?? []) {
          if (h.enabled && h.name)
            rows.set(h.name.toLowerCase(), {
              name: h.name,
              value: h.value,
              source: f.node?.name ?? '',
            });
        }
      }
      folderHeaders = [...rows.values()];
    });
  });

  const autoHeaders = $derived.by<InheritedRow[]>(() => {
    if (!req) return [];
    const has = (n: string) => req.headers.some((h) => h.enabled && h.name.toLowerCase() === n);
    const rows: InheritedRow[] = [];
    const contentType: Partial<Record<string, string>> = {
      json: 'application/json',
      xml: 'application/xml',
      yaml: 'application/yaml',
      text: 'text/plain',
      html: 'text/html',
      formUrlencoded: 'application/x-www-form-urlencoded',
      multipart: 'multipart/form-data; boundary=…',
    };
    const ct = contentType[req.body.kind];
    if (ct && !has('content-type'))
      rows.push({ name: 'Content-Type', value: ct, source: t('request.auto') });
    if (!has('user-agent'))
      rows.push({ name: 'User-Agent', value: 'reqly', source: t('request.auto') });
    return rows;
  });

  const inherited = $derived(
    [...folderHeaders, ...autoHeaders].filter(
      (h) => !req?.headers.some((r) => r.enabled && r.name.toLowerCase() === h.name.toLowerCase()),
    ),
  );

  function edit(
    mutate: (r: HttpRequest & { kind: 'request' }) => HttpRequest & { kind: 'request' },
  ) {
    if (!activeId) return;
    services.editor.editRequest(activeId, mutate);
    services.tabs.makePermanent(activeId);
  }

  function setUrl(url: string, params: KeyValue[]) {
    edit((r) => ({
      ...r,
      url,
      params,
      pathParams: syncPathParams(url, r.pathParams, () => crypto.randomUUID()),
    }));
  }

  async function importCurl(command: string) {
    const ok = await dialogs.confirm({
      title: t('request.curlTitle'),
      message: t('request.curlMessage'),
      confirmLabel: t('request.curlImport'),
    });
    if (!ok || !activeId) return;
    try {
      const [parsed] = await services.transport.call('curl.parse', { command });
      if (!parsed) return;
      edit((r) => ({ ...parsed.request, kind: 'request', description: r.description }));
      if (parsed.warnings.length) {
        toasts.show({
          message: t('request.curlWarnings', { list: parsed.warnings.join(', ') }),
          kind: 'warning',
          timeoutMs: 8000,
        });
      }
    } catch (e) {
      toasts.error(errorMessage(e));
    }
  }

  function send() {
    if (!activeId) return;
    const kind = streamKind(services.editor.get(activeId)?.node?.data);
    if (!kind) return void services.responses.send(activeId);
    // WebSocket and gRPC panels handle their own keys (Ctrl+Enter sends a message).
    if (kind === 'webSocket' || kind === 'grpc') return;
    const stream = services.streams.run(activeId);
    if (stream.open) void services.streams.disconnect(activeId);
    else void services.streams.connect(activeId);
  }

  onMount(() => {
    const offs = [
      services.hotkeys.on('request.send', send),
      services.hotkeys.on('request.save', () => activeId && void services.editor.save(activeId)),
      services.hotkeys.on('url.focus', () => urlBar?.focusUrl()),
    ];
    return () => offs.forEach((off) => off());
  });

  const tabs = $derived([
    {
      id: 'params' as const,
      label: t('request.params'),
      count: req ? req.params.filter((p) => p.enabled && p.name).length + req.pathParams.length : 0,
    },
    { id: 'body' as const, label: t('request.body'), dot: req ? req.body.kind !== 'none' : false },
    {
      id: 'auth' as const,
      label: t('request.auth'),
      dot: req ? req.auth.type !== 'inherit' && req.auth.type !== 'none' : false,
    },
    {
      id: 'headers' as const,
      label: t('request.headers'),
      count: req ? req.headers.filter((h) => h.enabled && h.name).length : 0,
    },
    {
      id: 'scripts' as const,
      label: t('scripts.title'),
      dot: req
        ? req.scripts.preRequest.trim() !== '' || req.scripts.afterResponse.trim() !== ''
        : false,
    },
    {
      id: 'variables' as const,
      label: t('request.variables'),
      count: req ? Object.keys(req.variables).length : 0,
    },
    { id: 'settings' as const, label: t('request.settings') },
    {
      id: 'docs' as const,
      label: t('request.docs'),
      dot: req ? req.description.trim() !== '' : false,
    },
  ]);
</script>

{#snippet valueCell(row: KeyValue, set: (value: string) => void)}
  <TemplateInput value={row.value} label={t('kv.value')} nodeId={activeId} onchange={set} />
{/snippet}

{#if !activeId}
  <EmptyState title={t('request.empty')} />
{:else if draft?.webSocket}
  <WebSocketPanel {services} id={activeId} ws={draft.webSocket} />
{:else if draft?.grpc}
  <GrpcPanel {services} id={activeId} grpc={draft.grpc} />
{:else if !req || !run}
  <div class="loading"><Spinner /></div>
{:else}
  <div class="request">
    <UrlBar
      bind:this={urlBar}
      nodeId={activeId}
      method={req.method}
      url={req.url}
      params={req.params}
      sending={streamRun ? streamRun.open : run.sending}
      {streaming}
      onmethod={(method) => edit((r) => ({ ...r, method }))}
      onurl={setUrl}
      onsend={send}
      oncancel={() =>
        activeId &&
        (streaming
          ? void services.streams.disconnect(activeId)
          : void services.responses.cancel(activeId))}
      oncurl={(c) => void importCurl(c)}
      oncode={() => activeId && interopDialogs.openCodegen(activeId)}
    />
    <div class="tabs">
      <Tabs
        label={t('request.sections')}
        idPrefix="request"
        {tabs}
        bind:value={section}
        onchange={(s) => activeId && sections.set(activeId, s)}
      />
    </div>
    <div
      class="panel"
      role="tabpanel"
      id="request-panel-{section}"
      aria-labelledby="request-tab-{section}"
    >
      {#if section === 'params'}
        <KeyValueEditor
          label={t('request.queryParams')}
          rows={req.params}
          {valueCell}
          onchange={(params) => edit((r) => ({ ...r, params }))}
        />
        {#if req.pathParams.length}
          <h3 class="subhead">{t('request.pathParams')}</h3>
          <KeyValueEditor
            label={t('request.pathParams')}
            rows={req.pathParams}
            {valueCell}
            onchange={(pathParams) => edit((r) => ({ ...r, pathParams }))}
          />
        {/if}
      {:else if section === 'body'}
        <BodyPanel
          {services}
          nodeId={activeId}
          body={req.body}
          {valueCell}
          onchange={(body) => edit((r) => ({ ...r, body }))}
        />
      {:else if section === 'auth'}
        <AuthPanel
          auth={req.auth}
          nodeId={activeId}
          transport={services.transport}
          onchange={(auth) => edit((r) => ({ ...r, auth }))}
        />
      {:else if section === 'headers'}
        <KeyValueEditor
          label={t('request.headers')}
          rows={req.headers}
          nameSuggestions={COMMON_HEADERS}
          {inherited}
          {valueCell}
          onchange={(headers) => edit((r) => ({ ...r, headers }))}
        />
      {:else if section === 'scripts'}
        <ScriptsPanel
          nodeId={activeId}
          scripts={req.scripts}
          onchange={(scripts) => edit((r) => ({ ...r, scripts }))}
        />
      {:else if section === 'variables'}
        <VariablesEditor
          label={t('request.variables')}
          data={req.variables}
          nodeId={activeId}
          onchange={(variables) => edit((r) => ({ ...r, variables }))}
        />
      {:else if section === 'settings'}
        <SettingsPanel
          settings={req.settings}
          onchange={(settings) => edit((r) => ({ ...r, settings }))}
        />
      {:else}
        <DocsPanel
          text={req.description}
          onchange={(description) => edit((r) => ({ ...r, description }))}
        />
      {/if}
    </div>
  </div>
{/if}

<style>
  .request {
    height: 100%;
    display: flex;
    flex-direction: column;
    min-height: 0;
  }
  .loading {
    height: 100%;
    display: grid;
    place-items: center;
    color: var(--c-fg-subtle);
  }
  .tabs {
    padding: 0 var(--space-2);
    border-bottom: 1px solid var(--c-border);
  }
  .panel {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: var(--space-3);
  }
  .subhead {
    margin: var(--space-4) 0 var(--space-2);
    font-size: var(--font-size-sm);
    font-weight: 600;
    color: var(--c-fg-muted);
  }
</style>
