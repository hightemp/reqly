<!--
  Edits the arguments of a template tag with a live preview of its value.
-->
<script lang="ts">
  import type { FunctionInfo } from '$lib/api/generated/FunctionInfo';
  import { t } from '$lib/i18n';
  import type { Services } from '$lib/services.svelte';
  import { parseTag, serializeTag, type ArgKind, type ParsedTag } from '$lib/template/scan';
  import Button from '$lib/ui/Button.svelte';
  import IconButton from '$lib/ui/IconButton.svelte';
  import Input from '$lib/ui/Input.svelte';
  import Combobox from '$lib/ui/overlay/Combobox.svelte';
  import Modal from '$lib/ui/overlay/Modal.svelte';
  import Select from '$lib/ui/overlay/Select.svelte';
  import type { TagEditRequest } from './tagEditorState.svelte';

  let {
    services,
    request,
    onclose,
  }: { services: Services; request: TagEditRequest; onclose: () => void } = $props();
  const envs = $derived(services.environments);

  // svelte-ignore state_referenced_locally
  let tag = $state<ParsedTag>(parseTag(request.text) ?? { name: 'uuid', args: [] });
  let functions = $state<FunctionInfo[]>([]);
  let faker = $state<string[]>([]);
  let preview = $state<{ ok: boolean; text: string } | null>(null);

  $effect(() => {
    void envs.loadCatalog().then((c) => {
      functions = c.functions;
      faker = c.faker.map((f) => f.name);
    });
  });

  const info = $derived(functions.find((f) => f.name === tag.name));
  const text = $derived(serializeTag(tag));
  const kinds: { value: ArgKind; label: string }[] = $derived([
    { value: 'string', label: t('tagEditor.kindString') },
    { value: 'number', label: t('tagEditor.kindNumber') },
    { value: 'boolean', label: t('tagEditor.kindBoolean') },
    { value: 'variable', label: t('tagEditor.kindVariable') },
  ]);

  // Live preview, debounced.
  $effect(() => {
    const current = text;
    const timer = setTimeout(() => {
      envs.preview(current, request.nodeId).then(
        (v) => (preview = { ok: true, text: v }),
        (e: unknown) => (preview = { ok: false, text: e instanceof Error ? e.message : String(e) }),
      );
    }, 250);
    return () => clearTimeout(timer);
  });

  function setArg(i: number, patch: Partial<{ kind: ArgKind; value: string }>) {
    const args = [...tag.args];
    while (args.length <= i) args.push({ kind: 'string', value: '' });
    args[i] = { ...args[i]!, ...patch };
    tag = { ...tag, args };
  }

  function argName(i: number) {
    return info?.args[i]?.replace(/\?$/, '') ?? t('tagEditor.argument', { n: i + 1 });
  }

  const argCount = $derived(Math.max(tag.args.length, info?.args.length ?? 0));

  function choicesFor(i: number): string[] | null {
    if (i !== 0) return null;
    if (tag.name === 'faker') return faker;
    return info?.choices.length ? info.choices : null;
  }

  function apply() {
    // Drop trailing empty optional arguments.
    const args = [...tag.args];
    while (args.length && args.at(-1)!.kind === 'string' && args.at(-1)!.value === '') args.pop();
    request.apply(serializeTag({ ...tag, args }));
    onclose();
  }
</script>

<Modal
  title={t('tagEditor.title')}
  width="560px"
  {onclose}
  initialFocus={() => document.querySelector<HTMLElement>('[data-tag-apply]')}
>
  <div class="form">
    <div class="field">
      <span class="label">{t('tagEditor.function')}</span>
      <Combobox
        label={t('tagEditor.function')}
        value={tag.name}
        options={functions.map((f) => f.name)}
        allowCustom
        mono
        onchange={(name) => (tag = { name, args: [] })}
      />
    </div>
    {#each { length: argCount } as _, i (i)}
      {@const arg = tag.args[i] ?? { kind: 'string', value: '' }}
      {@const choices = choicesFor(i)}
      <div class="field">
        <span class="label">{argName(i)}</span>
        <div class="arg">
          {#if choices && arg.kind === 'string'}
            <Combobox
              label={argName(i)}
              value={arg.value}
              options={choices}
              allowCustom
              mono
              onchange={(value) => setArg(i, { value })}
            />
          {:else}
            <Input
              mono
              value={arg.value}
              aria-label={argName(i)}
              oninput={(e) => setArg(i, { value: e.currentTarget.value })}
            />
          {/if}
          <div class="kind">
            <Select
              compact
              label={t('tagEditor.kind')}
              value={arg.kind}
              options={kinds}
              onchange={(kind) => setArg(i, { kind })}
            />
          </div>
          <IconButton
            icon="trash"
            label={t('tagEditor.removeArgument')}
            onclick={() => (tag = { ...tag, args: tag.args.filter((_, j) => j !== i) })}
          />
        </div>
      </div>
    {/each}
    <div>
      <Button onclick={() => setArg(tag.args.length, { kind: 'string', value: '' })}>
        {t('tagEditor.addArgument')}
      </Button>
    </div>
    {#if info?.example}
      <p class="example"><span>{t('tagEditor.example')}</span> <code>{info.example}</code></p>
    {/if}
    <div class="field">
      <span class="label">{t('tagEditor.template')}</span>
      <code class="code">{text}</code>
    </div>
    <div class="field">
      <span class="label">{t('tagEditor.preview')}</span>
      <output class="code" class:error={preview && !preview.ok} aria-live="polite">
        {preview ? preview.text || t('template.empty') : '…'}
      </output>
    </div>
  </div>
  {#snippet footer()}
    <Button onclick={onclose}>{t('common.cancel')}</Button>
    <Button variant="primary" data-tag-apply onclick={apply}>{t('tagEditor.apply')}</Button>
  {/snippet}
</Modal>

<style>
  .form {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .label {
    font-size: var(--font-size-xs);
    color: var(--c-fg-muted);
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
  .example {
    margin: 0;
    font-size: var(--font-size-sm);
    color: var(--c-fg-muted);
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
  .code.error {
    color: var(--c-danger);
  }
</style>
