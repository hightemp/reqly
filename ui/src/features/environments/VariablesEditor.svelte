<!--
  Variables as a table (name / value / secret) or as JSON. Used for
  environments and for folder / request variables.
-->
<script lang="ts">
  import { EditorView } from '@codemirror/view';
  import { tick } from 'svelte';
  import { t } from '$lib/i18n';
  import CodeEditor from '$lib/ui/code/CodeEditor.svelte';
  import TemplateInput from '$lib/ui/code/TemplateInput.svelte';
  import IconButton from '$lib/ui/IconButton.svelte';
  import Input from '$lib/ui/Input.svelte';
  import { fromRows, isPlainName, parseJsonData, toRows, type VarRow } from './vars';

  interface Props {
    data: Record<string, unknown>;
    secrets?: string[];
    /** Show the secret toggle (environments only). */
    allowSecrets?: boolean;
    /** Request or folder whose variables apply to values. */
    nodeId?: string | null;
    label: string;
    onchange: (data: Record<string, unknown>, secrets: string[]) => void;
  }

  let {
    data,
    secrets = [],
    allowSecrets = false,
    nodeId = null,
    label,
    onchange,
  }: Props = $props();

  const newId = () => crypto.randomUUID();
  let mode = $state<'table' | 'json'>('table');
  let rows = $state<VarRow[]>([]);
  let jsonText = $state('');
  let jsonError = $state<string | null>(null);
  let ghostName = $state('');
  let ghostValue = $state('');
  /** The data we last emitted, to tell our own echoes from external changes. */
  let emitted: string | null = null;

  $effect(() => {
    const key = JSON.stringify([data, secrets]);
    if (key === emitted) return;
    rows = toRows(data, secrets, newId);
    jsonText = JSON.stringify(data, null, 2);
    jsonError = null;
  });

  function emitRows(next: VarRow[]) {
    rows = next;
    const out = fromRows(next);
    emitted = JSON.stringify([out.data, out.secrets]);
    onchange(out.data, out.secrets);
  }

  function update(index: number, patch: Partial<VarRow>) {
    emitRows(rows.map((r, i) => (i === index ? { ...r, ...patch } : r)));
  }

  let rowsEl: HTMLDivElement | null = $state(null);

  async function addFromGhost(field: 'name' | 'value') {
    if (!ghostName && !ghostValue) return;
    emitRows([
      ...rows,
      { id: newId(), name: ghostName, value: ghostValue, json: false, secret: false },
    ]);
    ghostName = '';
    ghostValue = '';
    // Keep typing in the new row.
    await tick();
    const row = rowsEl?.querySelectorAll<HTMLElement>('[data-var-row]');
    const last = row?.[row.length - 1];
    if (field === 'name') {
      const input = last?.querySelector<HTMLInputElement>('input');
      input?.focus();
      input?.setSelectionRange(input.value.length, input.value.length);
    } else {
      const content = last?.querySelector<HTMLElement>('.cm-content');
      const view = content ? EditorView.findFromDOM(content) : null;
      view?.focus();
      view?.dispatch({ selection: { anchor: view.state.doc.length } });
    }
  }

  function onJson(text: string) {
    jsonText = text;
    const parsed = parseJsonData(text);
    if (!parsed.ok) {
      jsonError = parsed.error === 'object' ? t('variables.jsonObject') : parsed.error;
      return;
    }
    jsonError = null;
    const kept = secrets.filter((s) => s in parsed.data);
    emitted = JSON.stringify([parsed.data, kept]);
    onchange(parsed.data, kept);
  }

  function switchMode() {
    if (mode === 'table') jsonText = JSON.stringify(fromRows(rows).data, null, 2);
    else rows = toRows(data, secrets, newId);
    jsonError = null;
    mode = mode === 'table' ? 'json' : 'table';
  }
</script>

<div class="vars" data-vars={label}>
  <div class="toolbar">
    <span class="hint">{t('variables.hint')}</span>
    <button type="button" class="link" onclick={switchMode}>
      {mode === 'table' ? t('variables.jsonMode') : t('variables.tableMode')}
    </button>
  </div>

  {#if mode === 'table'}
    <div class="rows" role="list" aria-label={label} bind:this={rowsEl}>
      {#each rows as row, i (row.id)}
        <div class="row" role="listitem" data-var-row>
          <div class="name">
            <Input
              mono
              value={row.name}
              aria-label={t('variables.name')}
              invalid={row.name !== '' && !isPlainName(row.name)}
              oninput={(e) => update(i, { name: e.currentTarget.value })}
            />
          </div>
          <div class="value">
            <TemplateInput
              value={row.value}
              label={t('variables.value')}
              {nodeId}
              masked={row.secret}
              onchange={(v) => update(i, { value: v })}
            />
          </div>
          {#if allowSecrets}
            <IconButton
              icon={row.secret ? 'lock' : 'unlock'}
              label={row.secret ? t('variables.secretOn') : t('variables.secretOff')}
              pressed={row.secret}
              onclick={() => update(i, { secret: !row.secret })}
            />
          {/if}
          <IconButton
            icon="trash"
            label={t('kv.deleteRow')}
            onclick={() => emitRows(rows.filter((_, j) => j !== i))}
          />
        </div>
      {/each}
      <div class="row ghost">
        <div class="name">
          <Input
            mono
            bind:value={ghostName}
            aria-label={t('variables.newName')}
            placeholder={t('variables.name')}
            oninput={() => void addFromGhost('name')}
          />
        </div>
        <div class="value">
          <Input
            mono
            bind:value={ghostValue}
            aria-label={t('variables.newValue')}
            placeholder={t('variables.value')}
            oninput={() => void addFromGhost('value')}
          />
        </div>
        {#if allowSecrets}<span class="spacer"></span>{/if}
        <span class="spacer"></span>
      </div>
    </div>
  {:else}
    <div class="json">
      <CodeEditor value={jsonText} language="json" {label} templates {nodeId} onchange={onJson} />
    </div>
    {#if jsonError}
      <p class="error" role="alert">{jsonError}</p>
    {/if}
  {/if}
</div>

<style>
  .vars {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    min-height: 0;
    flex: 1;
  }
  .toolbar {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    font-size: var(--font-size-xs);
  }
  .hint {
    flex: 1;
    color: var(--c-fg-subtle);
  }
  .link {
    color: var(--c-fg-muted);
  }
  .link:hover {
    color: var(--c-fg);
  }
  .rows {
    display: flex;
    flex-direction: column;
    gap: 2px;
    overflow: auto;
  }
  .row {
    display: flex;
    align-items: center;
    gap: var(--space-1);
  }
  .name {
    flex: 2;
    min-width: 0;
  }
  .value {
    flex: 3;
    min-width: 0;
    display: flex;
  }
  .spacer {
    flex: none;
    width: var(--control-height);
  }
  .json {
    flex: 1;
    min-height: 160px;
    border: 1px solid var(--c-border);
    border-radius: var(--radius-sm);
    background: var(--c-bg-sunken);
  }
  .error {
    margin: 0;
    color: var(--c-danger);
    font-size: var(--font-size-sm);
  }
</style>
