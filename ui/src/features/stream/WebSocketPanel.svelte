<!-- WebSocket request: URL, connect/disconnect, message composer with saved messages, handshake settings. -->
<script lang="ts">
  import type { MessageFormat } from '$lib/api/generated/MessageFormat';
  import type { MessageTemplate } from '$lib/api/generated/MessageTemplate';
  import { t } from '$lib/i18n';
  import type { Services } from '$lib/services.svelte';
  import type { WebSocketData } from '$lib/stores/editor.svelte';
  import Button from '$lib/ui/Button.svelte';
  import Checkbox from '$lib/ui/Checkbox.svelte';
  import CodeEditor from '$lib/ui/code/CodeEditor.svelte';
  import TemplateInput from '$lib/ui/code/TemplateInput.svelte';
  import IconButton from '$lib/ui/IconButton.svelte';
  import Input from '$lib/ui/Input.svelte';
  import KeyValueEditor from '$lib/ui/kv/KeyValueEditor.svelte';
  import { dialogs } from '$lib/ui/overlay/dialogs.svelte';
  import Select from '$lib/ui/overlay/Select.svelte';
  import Tabs from '$lib/ui/Tabs.svelte';
  import VariablesEditor from '../environments/VariablesEditor.svelte';
  import { COMMON_HEADERS } from '../request/headers';
  import AuthPanel from '../request/panels/AuthPanel.svelte';
  import DocsPanel from '../request/panels/DocsPanel.svelte';
  import SettingsPanel from '../request/panels/SettingsPanel.svelte';

  interface Props {
    services: Services;
    id: string;
    ws: WebSocketData;
  }

  let { services, id, ws }: Props = $props();
  // Protocol tokens and examples, not translatable text.
  const BADGE = 'WS';
  const URL_EXAMPLE = 'wss://echo.websocket.org';
  const SUBPROTOCOL_EXAMPLE = 'graphql-transport-ws, chat';

  type Section = 'message' | 'params' | 'headers' | 'auth' | 'settings' | 'variables' | 'docs';
  let section = $state<Section>('message');
  const run = $derived(services.streams.run(id));
  const connected = $derived(run.open);

  function edit(mutate: (w: WebSocketData) => WebSocketData) {
    services.editor.editData<WebSocketData>(id, 'webSocket', mutate);
    services.tabs.makePermanent(id);
  }

  function toggle() {
    if (connected) void services.streams.disconnect(id);
    else void services.streams.connect(id);
  }

  async function send() {
    if (!connected) return;
    await services.streams.send(id, ws.draft.format, ws.draft.body);
  }

  async function saveTemplate() {
    const name = await dialogs.prompt({
      title: t('ws.saveTitle'),
      label: t('actions.name'),
      value: ws.draft.name || t('ws.messageName', { n: ws.templates.length + 1 }),
      validate: (v) => (v.trim() ? null : t('actions.nameRequired')),
    });
    if (!name) return;
    const template: MessageTemplate = { ...ws.draft, id: crypto.randomUUID(), name: name.trim() };
    edit((w) => ({ ...w, templates: [...w.templates, template] }));
  }

  function loadTemplate(m: MessageTemplate) {
    edit((w) => ({ ...w, draft: { ...m, id: w.draft.id } }));
  }

  function removeTemplate(templateId: string) {
    edit((w) => ({ ...w, templates: w.templates.filter((m) => m.id !== templateId) }));
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      void send();
    }
  }

  const formats: { value: MessageFormat; label: string }[] = [
    { value: 'text', label: 'Text' },
    { value: 'json', label: 'JSON' },
    { value: 'binary', label: 'Binary (base64)' },
  ];

  const tabs = $derived([
    { id: 'message' as const, label: t('ws.message') },
    {
      id: 'params' as const,
      label: t('request.params'),
      count: ws.params.filter((p) => p.enabled && p.name).length,
    },
    {
      id: 'headers' as const,
      label: t('request.headers'),
      count: ws.headers.filter((h) => h.enabled && h.name).length,
    },
    {
      id: 'auth' as const,
      label: t('request.auth'),
      dot: ws.auth.type !== 'inherit' && ws.auth.type !== 'none',
    },
    { id: 'settings' as const, label: t('request.settings') },
    {
      id: 'variables' as const,
      label: t('request.variables'),
      count: Object.keys(ws.variables).length,
    },
    { id: 'docs' as const, label: t('request.docs'), dot: ws.description.trim() !== '' },
  ]);
</script>

<div class="ws">
  <div class="bar">
    <span class="badge">{BADGE}</span>
    <div class="url">
      <TemplateInput
        value={ws.url}
        label={t('request.url')}
        placeholder={URL_EXAMPLE}
        nodeId={id}
        onchange={(url) => edit((w) => ({ ...w, url }))}
        onenter={toggle}
      />
    </div>
    <Button variant={connected ? 'default' : 'primary'} loading={run.connecting} onclick={toggle}
      >{connected ? t('stream.disconnect') : t('stream.connect')}</Button
    >
  </div>
  <div class="tabs">
    <Tabs label={t('request.sections')} idPrefix="ws" {tabs} bind:value={section} />
  </div>
  <div class="panel" role="tabpanel" id="ws-panel-{section}" aria-labelledby="ws-tab-{section}">
    {#if section === 'message'}
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="composer" {onkeydown}>
        <div class="toolbar">
          <Select
            label={t('ws.format')}
            compact
            value={ws.draft.format}
            options={formats}
            onchange={(format) => edit((w) => ({ ...w, draft: { ...w.draft, format } }))}
          />
          <span class="spacer"></span>
          <Button size="sm" variant="ghost" onclick={() => void saveTemplate()}
            >{t('ws.save')}</Button
          >
          <Button size="sm" variant="primary" disabled={!connected} onclick={() => void send()}
            >{t('ws.send')}</Button
          >
        </div>
        <div class="editor">
          <CodeEditor
            label={t('ws.message')}
            value={ws.draft.body}
            language={ws.draft.format === 'json' ? 'json' : 'text'}
            templates
            nodeId={id}
            placeholder={t('ws.placeholder')}
            onchange={(body) => edit((w) => ({ ...w, draft: { ...w.draft, body } }))}
          />
        </div>
        {#if ws.templates.length}
          <div class="saved">
            <h3>{t('ws.saved')}</h3>
            <ul>
              {#each ws.templates as m (m.id)}
                <li>
                  <button type="button" class="saved-item" onclick={() => loadTemplate(m)}>
                    <span class="name">{m.name}</span>
                    <span class="fmt">{m.format}</span>
                  </button>
                  <IconButton
                    icon="trash"
                    label={t('ws.remove')}
                    onclick={() => removeTemplate(m.id)}
                  />
                </li>
              {/each}
            </ul>
          </div>
        {/if}
      </div>
    {:else if section === 'params'}
      <KeyValueEditor
        label={t('request.queryParams')}
        rows={ws.params}
        onchange={(params) => edit((w) => ({ ...w, params }))}
      />
    {:else if section === 'headers'}
      <KeyValueEditor
        label={t('request.headers')}
        rows={ws.headers}
        nameSuggestions={COMMON_HEADERS}
        onchange={(headers) => edit((w) => ({ ...w, headers }))}
      />
    {:else if section === 'auth'}
      <AuthPanel
        auth={ws.auth}
        nodeId={id}
        transport={services.transport}
        onchange={(auth) => edit((w) => ({ ...w, auth }))}
      />
    {:else if section === 'settings'}
      <div class="ws-settings">
        <label class="row">
          <span class="label">{t('ws.subprotocols')}</span>
          <Input
            value={ws.subprotocols.join(', ')}
            placeholder={SUBPROTOCOL_EXAMPLE}
            aria-label={t('ws.subprotocols')}
            onchange={(e) =>
              edit((w) => ({
                ...w,
                subprotocols: e.currentTarget.value
                  .split(',')
                  .map((p) => p.trim())
                  .filter(Boolean),
              }))}
          />
        </label>
        <div class="row">
          <span></span>
          <Checkbox
            checked={ws.autoReconnect}
            label={t('ws.autoReconnect')}
            showLabel
            onchange={(autoReconnect) => edit((w) => ({ ...w, autoReconnect }))}
          />
        </div>
        <label class="row">
          <span class="label">{t('ws.reconnectDelay')}</span>
          <Input
            type="number"
            min="100"
            value={String(ws.reconnectDelayMs)}
            aria-label={t('ws.reconnectDelay')}
            onchange={(e) =>
              edit((w) => ({
                ...w,
                reconnectDelayMs: Math.max(100, Math.floor(Number(e.currentTarget.value) || 0)),
              }))}
          />
        </label>
      </div>
      <SettingsPanel
        settings={ws.settings}
        onchange={(settings) => edit((w) => ({ ...w, settings }))}
      />
    {:else if section === 'variables'}
      <VariablesEditor
        label={t('request.variables')}
        data={ws.variables}
        nodeId={id}
        onchange={(variables) => edit((w) => ({ ...w, variables }))}
      />
    {:else}
      <DocsPanel
        text={ws.description}
        onchange={(description) => edit((w) => ({ ...w, description }))}
      />
    {/if}
  </div>
</div>

<style>
  .ws {
    height: 100%;
    display: flex;
    flex-direction: column;
    min-height: 0;
  }
  .bar {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-3);
    border-bottom: 1px solid var(--c-border);
  }
  .badge {
    flex: none;
    font-family: var(--font-mono);
    font-weight: 700;
    font-size: var(--font-size-sm);
    color: var(--c-protocol-ws, #14b8a6);
    width: 40px;
  }
  .url {
    flex: 1;
    min-width: 0;
    display: flex;
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
  .composer {
    height: 100%;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    min-height: 0;
  }
  .toolbar {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .spacer {
    flex: 1;
  }
  .editor {
    flex: 1;
    min-height: 120px;
    border: 1px solid var(--c-border);
    border-radius: var(--radius-sm);
    overflow: hidden;
  }
  .saved h3 {
    font-size: var(--font-size-sm);
    color: var(--c-fg-muted);
    margin-bottom: var(--space-1);
  }
  .saved ul {
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: 2px;
    max-height: 160px;
    overflow: auto;
  }
  .saved li {
    display: flex;
    align-items: center;
  }
  .saved-item {
    flex: 1;
    display: flex;
    gap: var(--space-2);
    text-align: left;
    padding: var(--space-1) var(--space-2);
    border-radius: var(--radius-sm);
  }
  .saved-item:hover {
    background: var(--c-bg-hover);
  }
  .fmt {
    color: var(--c-fg-subtle);
    font-size: var(--font-size-xs);
  }
  .ws-settings {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    max-width: 640px;
    margin-bottom: var(--space-4);
  }
  .row {
    display: grid;
    grid-template-columns: 220px 1fr;
    align-items: center;
    gap: var(--space-3);
  }
  .label {
    color: var(--c-fg-muted);
  }
</style>
