<!-- gRPC request: address, service/method, message, .proto sources or reflection, metadata. -->
<script lang="ts">
  import type { GrpcMethodInfo } from '$lib/api/generated/GrpcMethodInfo';
  import type { GrpcServiceInfo } from '$lib/api/generated/GrpcServiceInfo';
  import type { InlineProto } from '$lib/api/generated/InlineProto';
  import { isTauri } from '$lib/api/transport';
  import { errorMessage } from '$lib/errors';
  import { t } from '$lib/i18n';
  import type { Services } from '$lib/services.svelte';
  import type { GrpcData } from '$lib/stores/editor.svelte';
  import Button from '$lib/ui/Button.svelte';
  import Checkbox from '$lib/ui/Checkbox.svelte';
  import CodeEditor from '$lib/ui/code/CodeEditor.svelte';
  import TemplateInput from '$lib/ui/code/TemplateInput.svelte';
  import IconButton from '$lib/ui/IconButton.svelte';
  import KeyValueEditor from '$lib/ui/kv/KeyValueEditor.svelte';
  import Select from '$lib/ui/overlay/Select.svelte';
  import Tabs from '$lib/ui/Tabs.svelte';
  import Textarea from '$lib/ui/Textarea.svelte';
  import VariablesEditor from '../environments/VariablesEditor.svelte';
  import AuthPanel from '../request/panels/AuthPanel.svelte';
  import DocsPanel from '../request/panels/DocsPanel.svelte';
  import SettingsPanel from '../request/panels/SettingsPanel.svelte';
  import { messageCompletions } from './messageCompletion';

  interface Props {
    services: Services;
    id: string;
    grpc: GrpcData;
  }

  let { services, id, grpc }: Props = $props();
  // Protocol tokens and examples, not translatable text.
  const BADGE = 'gRPC';
  const ADDRESS_EXAMPLE = 'grpcs://api.example.com:443';
  const PATH_EXAMPLE = '/path/to/protos';

  type Section = 'message' | 'proto' | 'metadata' | 'auth' | 'settings' | 'variables' | 'docs';
  let section = $state<Section>('message');
  let described = $state<GrpcServiceInfo[] | null>(null);
  let describing = $state(false);
  let describeError = $state<string | null>(null);
  let openProto = $state<number | null>(null);
  let fileInput: HTMLInputElement | undefined = $state();

  const run = $derived(services.streams.run(id));
  const active = $derived(run.open);
  const desktop = isTauri();

  const method = $derived<GrpcMethodInfo | null>(
    described?.find((s) => s.name === grpc.service)?.methods.find((m) => m.name === grpc.method) ??
      null,
  );
  const clientStreaming = $derived(method?.clientStreaming ?? false);
  // Field names from the method's example message.
  const completion = messageCompletions(() => method?.example ?? null);
  const hasSource = $derived(
    grpc.proto.reflection || grpc.proto.inline.length > 0 || grpc.proto.files.length > 0,
  );

  const methodOptions = $derived.by(() => {
    const options: { value: string; label: string; hint?: string }[] = [];
    for (const s of described ?? []) {
      for (const m of s.methods) {
        const kind = m.clientStreaming
          ? m.serverStreaming
            ? t('grpc.bidi')
            : t('grpc.clientStreaming')
          : m.serverStreaming
            ? t('grpc.serverStreaming')
            : t('grpc.unary');
        options.push({
          value: `${s.name}/${m.name}`,
          label: `${s.name.split('.').pop()}/${m.name}`,
          hint: kind,
        });
      }
    }
    const current = grpc.service ? `${grpc.service}/${grpc.method}` : '';
    if (current && !options.some((o) => o.value === current))
      options.unshift({ value: current, label: current });
    if (!current) options.unshift({ value: '', label: t('grpc.chooseMethod') });
    return options;
  });

  function edit(mutate: (g: GrpcData) => GrpcData) {
    services.editor.editData<GrpcData>(id, 'grpc', mutate);
    services.tabs.makePermanent(id);
  }

  async function describe() {
    if (!hasSource) return;
    describing = true;
    describeError = null;
    try {
      await services.editor.flush(id);
      described = await services.transport.call('grpc.describe', { requestId: id });
    } catch (e) {
      described = null;
      describeError = errorMessage(e);
    } finally {
      describing = false;
    }
  }

  // Describe once per request when sources exist.
  let describedFor = '';
  $effect(() => {
    const key = `${id}:${grpc.proto.reflection}:${grpc.proto.inline.length}:${grpc.proto.files.length}`;
    if (key === describedFor || !hasSource) return;
    describedFor = key;
    const timer = setTimeout(() => void describe(), 300);
    return () => clearTimeout(timer);
  });

  function chooseMethod(value: string) {
    const i = value.lastIndexOf('/');
    if (i < 0) return;
    const service = value.slice(0, i);
    const name = value.slice(i + 1);
    const info = described?.find((s) => s.name === service)?.methods.find((m) => m.name === name);
    edit((g) => ({
      ...g,
      service,
      method: name,
      // Fill an example when the message is still empty.
      message: info && (!g.message.trim() || g.message.trim() === '{}') ? info.example : g.message,
    }));
  }

  function toggle() {
    if (active) void services.streams.disconnect(id);
    else void services.streams.connect(id);
  }

  async function addFiles(files: FileList | null) {
    if (!files?.length) return;
    const added: InlineProto[] = [];
    for (const f of Array.from(files)) added.push({ name: f.name, text: await f.text() });
    edit((g) => {
      const keep = g.proto.inline.filter((p) => !added.some((a) => a.name === p.name));
      return { ...g, proto: { ...g.proto, inline: [...keep, ...added] } };
    });
    if (fileInput) fileInput.value = '';
  }

  function setInline(index: number, patch: Partial<InlineProto>) {
    edit((g) => ({
      ...g,
      proto: {
        ...g.proto,
        inline: g.proto.inline.map((p, i) => (i === index ? { ...p, ...patch } : p)),
      },
    }));
  }

  const tabs = $derived([
    { id: 'message' as const, label: t('grpc.message') },
    {
      id: 'proto' as const,
      label: t('grpc.proto'),
      count: grpc.proto.reflection ? 0 : grpc.proto.inline.length + grpc.proto.files.length,
      dot: grpc.proto.reflection,
    },
    {
      id: 'metadata' as const,
      label: t('grpc.metadata'),
      count: grpc.metadata.filter((m) => m.enabled && m.name).length,
    },
    {
      id: 'auth' as const,
      label: t('request.auth'),
      dot: grpc.auth.type !== 'inherit' && grpc.auth.type !== 'none',
    },
    { id: 'settings' as const, label: t('request.settings') },
    {
      id: 'variables' as const,
      label: t('request.variables'),
      count: Object.keys(grpc.variables).length,
    },
    { id: 'docs' as const, label: t('request.docs'), dot: grpc.description.trim() !== '' },
  ]);
</script>

<div class="grpc">
  <div class="bar">
    <span class="badge">{BADGE}</span>
    <div class="url">
      <TemplateInput
        value={grpc.url}
        label={t('grpc.address')}
        placeholder={ADDRESS_EXAMPLE}
        nodeId={id}
        onchange={(url) => edit((g) => ({ ...g, url }))}
        onenter={toggle}
      />
    </div>
    <div class="method">
      <Select
        label={t('grpc.method')}
        value={grpc.service ? `${grpc.service}/${grpc.method}` : ''}
        options={methodOptions}
        onchange={chooseMethod}
      />
    </div>
    <IconButton
      icon="refresh"
      label={t('grpc.refresh')}
      disabled={describing || !hasSource}
      onclick={() => void describe()}
    />
    <Button variant={active ? 'default' : 'primary'} loading={run.connecting} onclick={toggle}
      >{active ? t('request.cancel') : t('grpc.invoke')}</Button
    >
  </div>
  {#if describeError}
    <p class="error" role="alert">{describeError}</p>
  {:else if !hasSource}
    <p class="hint">{t('grpc.noSource')}</p>
  {/if}
  <div class="tabs">
    <Tabs label={t('request.sections')} idPrefix="grpc" {tabs} bind:value={section} />
  </div>
  <div class="panel" role="tabpanel" id="grpc-panel-{section}" aria-labelledby="grpc-tab-{section}">
    {#if section === 'message'}
      <div class="composer">
        <div class="toolbar">
          {#if method}
            <span class="hint">{method.inputType} → {method.outputType}</span>
          {/if}
          <span class="spacer"></span>
          {#if method}
            <Button
              size="sm"
              variant="ghost"
              onclick={() => edit((g) => ({ ...g, message: method?.example ?? g.message }))}
              >{t('grpc.example')}</Button
            >
          {/if}
          {#if clientStreaming && active}
            <Button
              size="sm"
              variant="primary"
              onclick={() => void services.streams.send(id, 'json', grpc.message)}
              >{t('grpc.sendMessage')}</Button
            >
            <Button size="sm" onclick={() => void services.streams.end(id)}
              >{t('grpc.endStream')}</Button
            >
          {/if}
        </div>
        {#if clientStreaming}
          <p class="hint">{t('grpc.streamingHint')}</p>
        {/if}
        <div class="editor">
          <CodeEditor
            label={t('grpc.message')}
            value={grpc.message}
            language="json"
            completions={[completion]}
            templates
            nodeId={id}
            onchange={(message) => edit((g) => ({ ...g, message }))}
          />
        </div>
      </div>
    {:else if section === 'proto'}
      <div class="proto">
        <Checkbox
          checked={grpc.proto.reflection}
          label={t('grpc.reflection')}
          showLabel
          onchange={(reflection) => edit((g) => ({ ...g, proto: { ...g.proto, reflection } }))}
        />
        {#if !grpc.proto.reflection}
          <div class="files">
            <div class="toolbar">
              <h3>{t('grpc.files')}</h3>
              <span class="spacer"></span>
              <input
                bind:this={fileInput}
                type="file"
                accept=".proto"
                multiple
                hidden
                onchange={(e) => void addFiles(e.currentTarget.files)}
              />
              <Button size="sm" onclick={() => fileInput?.click()}>{t('grpc.addFiles')}</Button>
              <Button
                size="sm"
                variant="ghost"
                onclick={() =>
                  edit((g) => ({
                    ...g,
                    proto: {
                      ...g.proto,
                      inline: [
                        ...g.proto.inline,
                        {
                          name: `file${g.proto.inline.length + 1}.proto`,
                          text: 'syntax = "proto3";\n',
                        },
                      ],
                    },
                  }))}>{t('grpc.newFile')}</Button
              >
            </div>
            {#if grpc.proto.inline.length === 0}
              <p class="hint">{t('grpc.filesHint')}</p>
            {/if}
            {#each grpc.proto.inline as file, i (i)}
              <div class="file">
                <div class="file-head">
                  <button
                    type="button"
                    class="file-toggle"
                    onclick={() => (openProto = openProto === i ? null : i)}
                    >{openProto === i ? '▾' : '▸'}</button
                  >
                  <input
                    class="file-name"
                    value={file.name}
                    aria-label={t('grpc.fileName')}
                    onchange={(e) => setInline(i, { name: e.currentTarget.value.trim() })}
                  />
                  <IconButton
                    icon="trash"
                    label={t('grpc.removeFile')}
                    onclick={() =>
                      edit((g) => ({
                        ...g,
                        proto: { ...g.proto, inline: g.proto.inline.filter((_, j) => j !== i) },
                      }))}
                  />
                </div>
                {#if openProto === i}
                  <div class="file-editor">
                    <CodeEditor
                      label={file.name}
                      value={file.text}
                      onchange={(text) => setInline(i, { text })}
                    />
                  </div>
                {/if}
              </div>
            {/each}
            {#if desktop}
              <label class="paths">
                <span class="label">{t('grpc.importPaths')}</span>
                <Textarea
                  mono
                  rows={3}
                  value={grpc.proto.importPaths.join('\n')}
                  placeholder={PATH_EXAMPLE}
                  aria-label={t('grpc.importPaths')}
                  onchange={(e) =>
                    edit((g) => ({
                      ...g,
                      proto: {
                        ...g.proto,
                        importPaths: e.currentTarget.value
                          .split('\n')
                          .map((p) => p.trim())
                          .filter(Boolean),
                      },
                    }))}
                />
              </label>
            {/if}
          </div>
        {/if}
      </div>
    {:else if section === 'metadata'}
      <KeyValueEditor
        label={t('grpc.metadata')}
        rows={grpc.metadata}
        onchange={(metadata) => edit((g) => ({ ...g, metadata }))}
      />
    {:else if section === 'auth'}
      <AuthPanel
        auth={grpc.auth}
        nodeId={id}
        transport={services.transport}
        onchange={(auth) => edit((g) => ({ ...g, auth }))}
      />
    {:else if section === 'settings'}
      <SettingsPanel
        settings={grpc.settings}
        onchange={(settings) => edit((g) => ({ ...g, settings }))}
      />
    {:else if section === 'variables'}
      <VariablesEditor
        label={t('request.variables')}
        data={grpc.variables}
        nodeId={id}
        onchange={(variables) => edit((g) => ({ ...g, variables }))}
      />
    {:else}
      <DocsPanel
        text={grpc.description}
        onchange={(description) => edit((g) => ({ ...g, description }))}
      />
    {/if}
  </div>
</div>

<style>
  .grpc {
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
    color: var(--c-protocol-grpc, #6366f1);
    width: 40px;
  }
  .url {
    flex: 1;
    min-width: 0;
    display: flex;
  }
  .method {
    width: 240px;
    flex: none;
  }
  .error,
  .hint {
    padding: var(--space-1) var(--space-3);
    font-size: var(--font-size-xs);
    color: var(--c-fg-subtle);
  }
  .error {
    color: var(--c-danger);
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
  .composer .hint {
    padding: 0;
  }
  .toolbar {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .toolbar h3 {
    font-size: var(--font-size-sm);
    font-weight: 600;
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
  .proto,
  .files {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    max-width: 900px;
  }
  .files .hint {
    padding: 0;
  }
  .file {
    border: 1px solid var(--c-border);
    border-radius: var(--radius-sm);
  }
  .file-head {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    padding: var(--space-1);
  }
  .file-toggle {
    width: 20px;
    color: var(--c-fg-muted);
  }
  .file-name {
    flex: 1;
    font-family: var(--font-mono);
    font-size: var(--font-size-sm);
    background: transparent;
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    padding: 2px var(--space-1);
    color: var(--c-fg);
  }
  .file-name:focus {
    border-color: var(--c-accent);
    outline: none;
  }
  .file-editor {
    height: 280px;
    border-top: 1px solid var(--c-border);
  }
  .paths {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }
  .label {
    color: var(--c-fg-muted);
  }
</style>
