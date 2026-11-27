<!--
  Edits a template tag: pick a function, then each argument with its own
  control (a list, a number, a request of the tree…), with a live preview.
-->
<script lang="ts">
  import type { ArgInfo } from '$lib/api/generated/ArgInfo';
  import type { FunctionInfo } from '$lib/api/generated/FunctionInfo';
  import { t } from '$lib/i18n';
  import type { Services } from '$lib/services.svelte';
  import { serializeTag, type ArgKind, type ParsedTag } from '$lib/template/scan';
  import Button from '$lib/ui/Button.svelte';
  import IconButton from '$lib/ui/IconButton.svelte';
  import Input from '$lib/ui/Input.svelte';
  import Combobox from '$lib/ui/overlay/Combobox.svelte';
  import Modal from '$lib/ui/overlay/Modal.svelte';
  import Select from '$lib/ui/overlay/Select.svelte';
  import { finishTag, openTag, requestOptions, startTag, visibleParams } from './tagModel';
  import type { TagEditRequest } from './tagEditorState.svelte';

  let {
    services,
    request,
    onclose,
  }: { services: Services; request: TagEditRequest; onclose: () => void } = $props();
  const envs = $derived(services.environments);

  let functions = $state<FunctionInfo[]>([]);
  let faker = $state<string[]>([]);
  let tag = $state<ParsedTag | null>(null);
  let preview = $state<{ ok: boolean; text: string } | null>(null);
  let refreshes = $state(0);

  $effect(() => {
    void envs.loadCatalog().then((c) => {
      functions = c.functions;
      faker = c.faker.map((f) => f.name);
      tag ??= openTag(request.text, c.functions);
    });
  });

  const info = $derived(tag ? functions.find((f) => f.name === tag!.name) : undefined);
  const text = $derived(tag ? serializeTag(finishTag(tag, info)) : '');
  const requests = $derived(requestOptions(services.tree.items));

  // Live preview, debounced; "Refresh" asks again (e.g. after sending).
  $effect(() => {
    const current = text;
    void refreshes;
    if (!current) return;
    const timer = setTimeout(() => {
      envs.preview(current, request.nodeId).then(
        (v) => (preview = { ok: true, text: v }),
        (e: unknown) => (preview = { ok: false, text: e instanceof Error ? e.message : String(e) }),
      );
    }, 250);
    return () => clearTimeout(timer);
  });

  const fnLabel = (name: string) => {
    // i18n-dynamic: tagEditor.fn.
    const key = `tagEditor.fn.${name}` as Parameters<typeof t>[0];
    const label = t(key);
    return label === key ? name : label;
  };

  function argLabel(p: ArgInfo): string {
    if (tag?.name === 'response' && p.name === 'path') {
      return valueAt(0) === 'header' ? t('tagEditor.arg.headerName') : t('tagEditor.arg.filter');
    }
    // i18n-dynamic: tagEditor.arg.
    const key = `tagEditor.arg.${p.name}` as Parameters<typeof t>[0];
    const label = t(key);
    return label === key ? p.name : label;
  }

  function choiceLabel(p: ArgInfo, value: string): string {
    if (tag?.name !== 'response') return value;
    // i18n-dynamic: tagEditor.responseField.
    // i18n-dynamic: tagEditor.trigger.
    if (p.name === 'field') return t(`tagEditor.responseField.${value}` as Parameters<typeof t>[0]);
    if (p.name === 'trigger') return t(`tagEditor.trigger.${value}` as Parameters<typeof t>[0]);
    return value;
  }

  const indexOf = (p: ArgInfo) => info?.params.indexOf(p) ?? -1;
  const valueAt = (i: number) => tag?.args[i]?.value ?? '';

  function setArg(i: number, patch: Partial<{ kind: ArgKind; value: string }>) {
    if (!tag) return;
    const args = [...tag.args];
    while (args.length <= i) args.push({ kind: 'string', value: '' });
    args[i] = { ...args[i]!, ...patch };
    tag = { ...tag, args };
  }

  function setFunction(name: string) {
    const next = functions.find((f) => f.name === name);
    tag = next ? startTag(next) : { name, args: [] };
  }

  function choices(p: ArgInfo) {
    const options = p.choices.map((value) => ({ value, label: choiceLabel(p, value) }));
    return p.optional ? [{ value: '', label: t('tagEditor.default') }, ...options] : options;
  }

  // The request picker shows paths; ids not in this collection stay as they are.
  function requestLabel(id: string) {
    if (!id) return '';
    return requests.find((r) => r.value === id)?.label ?? t('tagEditor.unknownRequest', { id });
  }
  function pickRequest(i: number, label: string) {
    const match = requests.find((r) => r.label === label);
    setArg(i, { kind: 'string', value: match ? match.value : label.trim() });
  }

  // Unknown functions and extra arguments: edit them as plain text.
  const extra = $derived(tag && info ? tag.args.slice(info.params.length) : []);

  function apply() {
    if (!tag) return;
    request.apply(serializeTag(finishTag(tag, info)));
    onclose();
  }
</script>

<Modal
  title={t('tagEditor.title')}
  width="600px"
  {onclose}
  initialFocus={() => document.querySelector<HTMLElement>('[data-tag-apply]')}
>
  {#if tag}
    <div class="form">
      <div class="field">
        <span class="label">{t('tagEditor.function')}</span>
        <Select
          label={t('tagEditor.function')}
          value={tag.name}
          options={functions.map((f) => ({ value: f.name, label: fnLabel(f.name) }))}
          onchange={setFunction}
        />
      </div>

      {#if info}
        {#each visibleParams(info, tag) as p (p.name)}
          {@const i = indexOf(p)}
          {@const arg = tag.args[i] ?? { kind: 'string', value: '' }}
          <div class="field">
            <span class="label">{argLabel(p)}</span>
            {#if p.kind === 'choice'}
              <Select
                label={argLabel(p)}
                value={arg.value}
                options={choices(p)}
                onchange={(value) => setArg(i, { kind: 'string', value })}
              />
            {:else if p.kind === 'request'}
              <Combobox
                label={argLabel(p)}
                value={requestLabel(arg.value)}
                options={requests.map((r) => r.label)}
                placeholder={t('tagEditor.pickRequest')}
                allowCustom
                onchange={(label) => pickRequest(i, label)}
              />
            {:else if p.kind === 'faker'}
              <Combobox
                label={argLabel(p)}
                value={arg.value}
                options={faker}
                allowCustom
                mono
                onchange={(value) => setArg(i, { kind: 'string', value })}
              />
            {:else}
              <div class="arg">
                <Input
                  mono
                  type={p.kind === 'number' ? 'number' : 'text'}
                  value={arg.value}
                  aria-label={argLabel(p)}
                  oninput={(e) => setArg(i, { value: e.currentTarget.value })}
                />
                {#if p.kind === 'text'}
                  <div class="kind">
                    <Select
                      compact
                      label={t('tagEditor.kind')}
                      value={arg.kind === 'variable' ? 'variable' : 'string'}
                      options={[
                        { value: 'string', label: t('tagEditor.kindString') },
                        { value: 'variable', label: t('tagEditor.kindVariable') },
                      ]}
                      onchange={(kind) => setArg(i, { kind })}
                    />
                  </div>
                {/if}
              </div>
            {/if}
            {#if tag.name === 'response' && p.name === 'path' && valueAt(0) === 'body'}
              <span class="hint">{t('tagEditor.filterHint')}</span>
            {/if}
          </div>
        {/each}
      {/if}

      {#if !info || extra.length}
        <details class="advanced" open={!info}>
          <summary>{t('tagEditor.advanced')}</summary>
          {#each tag.args as arg, i (i)}
            {#if !info || i >= info.params.length}
              <div class="arg">
                <Input
                  mono
                  value={arg.value}
                  aria-label={t('tagEditor.argument', { n: i + 1 })}
                  oninput={(e) => setArg(i, { value: e.currentTarget.value })}
                />
                <IconButton
                  icon="trash"
                  label={t('tagEditor.removeArgument')}
                  onclick={() => (tag = { ...tag!, args: tag!.args.filter((_, j) => j !== i) })}
                />
              </div>
            {/if}
          {/each}
          <Button size="sm" onclick={() => setArg(tag!.args.length, { value: '' })}>
            {t('tagEditor.addArgument')}
          </Button>
        </details>
      {/if}

      <div class="field">
        <span class="label">{t('tagEditor.template')}</span>
        <code class="code selectable">{text}</code>
      </div>
      <div class="field">
        <div class="row">
          <span class="label">{t('tagEditor.preview')}</span>
          <IconButton
            icon="refresh"
            size={14}
            label={t('tagEditor.refresh')}
            onclick={() => refreshes++}
          />
        </div>
        <output
          class="code preview selectable"
          class:error={preview && !preview.ok}
          aria-live="polite"
        >
          {preview ? preview.text || t('template.empty') : '…'}
        </output>
      </div>
    </div>
  {/if}
  {#snippet footer()}
    <Button onclick={onclose}>{t('common.cancel')}</Button>
    <Button variant="primary" data-tag-apply disabled={!tag} onclick={apply}
      >{t('tagEditor.apply')}</Button
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
    flex-direction: column;
    gap: 4px;
  }
  .label {
    font-size: var(--font-size-xs);
    font-weight: 600;
    color: var(--c-fg-muted);
  }
  .hint {
    font-size: var(--font-size-xs);
    color: var(--c-fg-subtle);
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .arg {
    display: flex;
    align-items: center;
    gap: var(--space-1);
  }
  .arg > :first-child {
    flex: 1;
    min-width: 0;
  }
  .kind {
    width: 120px;
    flex: none;
  }
  .advanced {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    font-size: var(--font-size-sm);
  }
  .advanced summary {
    cursor: pointer;
    color: var(--c-fg-muted);
    margin-bottom: var(--space-2);
  }
  .code {
    display: block;
    padding: var(--space-1) var(--space-2);
    border-radius: var(--radius-sm);
    background: var(--c-bg-sunken);
    font-family: var(--font-mono);
    font-size: var(--font-size-sm);
    white-space: pre-wrap;
    word-break: break-all;
  }
  .preview {
    min-height: 3.5em;
    max-height: 12em;
    overflow: auto;
  }
  .code.error {
    color: var(--c-danger);
  }
</style>
