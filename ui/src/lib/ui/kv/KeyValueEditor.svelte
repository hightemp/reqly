<!--
  Editable name/value rows (params, headers, form fields) with an always-present
  empty row for adding, drag reordering, and a bulk text mode.
-->
<script lang="ts" module>
  export interface InheritedRow {
    name: string;
    value: string;
    /** Where it comes from, e.g. folder name or "auto". */
    source: string;
  }
</script>

<script lang="ts">
  import type { Snippet } from 'svelte';
  import { t } from '$lib/i18n';
  import Checkbox from '../Checkbox.svelte';
  import IconButton from '../IconButton.svelte';
  import Input from '../Input.svelte';
  import Combobox from '../overlay/Combobox.svelte';
  import Select from '../overlay/Select.svelte';
  import { tooltip } from '../overlay/tooltip.svelte';
  import Textarea from '../Textarea.svelte';
  import { fromBulk, toBulk, type KvRow } from './bulk';

  interface Props {
    rows: KvRow[];
    label: string;
    namePlaceholder?: string;
    valuePlaceholder?: string;
    nameSuggestions?: string[];
    allowFiles?: boolean;
    separator?: ':' | '=';
    inherited?: InheritedRow[];
    /** Opens a file picker; returns the stored value (path or `blob:` ref) and display name. */
    pickFile?: () => Promise<{ value: string; name: string } | null>;
    /** Renders the value cell instead of a plain input (template-aware input in stage 2). */
    valueCell?: Snippet<[KvRow, (value: string) => void]>;
    onchange?: (rows: KvRow[]) => void;
  }

  let {
    rows = $bindable([]),
    label,
    namePlaceholder,
    valuePlaceholder,
    nameSuggestions,
    allowFiles = false,
    separator = ':',
    inherited = [],
    pickFile,
    valueCell,
    onchange,
  }: Props = $props();

  let bulk = $state(false);
  let bulkText = $state('');
  let showDescription = $state(false);
  let dragIndex = $state<number | null>(null);
  let overIndex = $state<number | null>(null);
  let ghostName = $state('');
  let ghostValue = $state('');

  const newId = () => crypto.randomUUID();

  function emit(next: KvRow[]) {
    rows = next;
    onchange?.(next);
  }

  function update(index: number, patch: Partial<KvRow>) {
    emit(rows.map((r, i) => (i === index ? { ...r, ...patch } : r)));
  }

  function remove(index: number) {
    emit(rows.filter((_, i) => i !== index));
  }

  function addFromGhost() {
    if (!ghostName && !ghostValue) return;
    emit([
      ...rows,
      {
        id: newId(),
        enabled: true,
        name: ghostName,
        value: ghostValue,
        description: '',
        kind: 'text',
      },
    ]);
    ghostName = '';
    ghostValue = '';
    // Keep typing in the new row's corresponding input.
    queueMicrotask(() => {
      const inputs = document.querySelectorAll<HTMLInputElement>(
        `[data-kv="${label}"] [data-row="${rows.length - 1}"] input`,
      );
      inputs[0]?.focus();
      const el = inputs[0];
      if (el) el.setSelectionRange(el.value.length, el.value.length);
    });
  }

  function toggleBulk() {
    if (bulk) {
      emit(fromBulk(bulkText, rows, newId, separator));
    } else {
      bulkText = toBulk(rows, separator);
    }
    bulk = !bulk;
  }

  async function chooseFile(index: number) {
    const picked = await pickFile?.();
    if (picked)
      update(index, { value: picked.value, description: rows[index]!.description || picked.name });
  }

  function onPointerMove(e: PointerEvent) {
    if (dragIndex === null) return;
    const el = document.elementFromPoint(e.clientX, e.clientY)?.closest<HTMLElement>('[data-row]');
    if (el?.closest(`[data-kv="${label}"]`)) overIndex = Number(el.dataset.row);
  }

  function onPointerUp() {
    if (dragIndex !== null && overIndex !== null && overIndex !== dragIndex) {
      const next = [...rows];
      const [moved] = next.splice(dragIndex, 1);
      next.splice(overIndex, 0, moved!);
      emit(next);
    }
    dragIndex = null;
    overIndex = null;
    window.removeEventListener('pointermove', onPointerMove);
    window.removeEventListener('pointerup', onPointerUp);
  }

  function startDrag(e: PointerEvent, index: number) {
    e.preventDefault();
    dragIndex = index;
    window.addEventListener('pointermove', onPointerMove);
    window.addEventListener('pointerup', onPointerUp);
  }

  const fileName = (value: string) => value.split(/[\\/]/).pop() ?? value;
</script>

<div class="kv" data-kv={label}>
  <div class="toolbar">
    <button type="button" class="link" onclick={toggleBulk}
      >{bulk ? t('kv.tableEdit') : t('kv.bulkEdit')}</button
    >
    {#if !bulk}
      <button type="button" class="link" onclick={() => (showDescription = !showDescription)}>
        {showDescription ? t('kv.hideDescription') : t('kv.showDescription')}
      </button>
      {#if rows.length > 0}
        <button type="button" class="link danger" onclick={() => emit([])}
          >{t('kv.deleteAll')}</button
        >
      {/if}
    {/if}
  </div>

  {#if bulk}
    <Textarea mono bind:value={bulkText} aria-label={label} rows={10} />
  {:else}
    <div class="rows" role="table" aria-label={label}>
      {#each inherited as row, i (i)}
        <div
          class="row inherited"
          role="row"
          use:tooltip={t('kv.inheritedFrom', { source: row.source })}
        >
          <span class="handle"></span>
          <span class="cell-check"></span>
          <span class="cell readonly mono" role="cell">{row.name}</span>
          <span class="cell inherited-value" role="cell">
            <span class="readonly mono">{row.value}</span>
            <span class="source">{row.source}</span>
          </span>
          {#if allowFiles}<span class="kind"></span>{/if}
          {#if showDescription}<span class="cell"></span>{/if}
          <span class="spacer"></span>
        </div>
      {/each}

      {#each rows as row, i (row.id)}
        <div
          class="row"
          role="row"
          data-row={i}
          class:disabled={!row.enabled}
          class:drop-target={overIndex === i && dragIndex !== i}
        >
          <span
            class="handle"
            role="button"
            tabindex="-1"
            aria-label={t('kv.dragHandle')}
            onpointerdown={(e) => startDrag(e, i)}
          >
            <svg viewBox="0 0 16 16" width="10" height="12" aria-hidden="true"
              ><circle cx="5" cy="4" r="1.2" /><circle cx="11" cy="4" r="1.2" /><circle
                cx="5"
                cy="8"
                r="1.2"
              /><circle cx="11" cy="8" r="1.2" /><circle cx="5" cy="12" r="1.2" /><circle
                cx="11"
                cy="12"
                r="1.2"
              /></svg
            >
          </span>
          <span class="cell-check">
            <Checkbox
              checked={row.enabled}
              label={t('kv.enabled')}
              onchange={(c) => update(i, { enabled: c })}
            />
          </span>
          <span class="cell" role="cell">
            {#if nameSuggestions}
              <Combobox
                value={row.name}
                options={nameSuggestions}
                allowCustom
                mono
                label={t('kv.name')}
                onchange={(v) => update(i, { name: v })}
              />
            {:else}
              <Input
                mono
                value={row.name}
                aria-label={t('kv.name')}
                placeholder={namePlaceholder}
                oninput={(e) => update(i, { name: e.currentTarget.value })}
              />
            {/if}
          </span>
          <span class="cell" role="cell">
            {#if row.kind === 'file'}
              <button type="button" class="file" onclick={() => chooseFile(i)}>
                {row.value ? fileName(row.description || row.value) : t('kv.chooseFile')}
              </button>
            {:else if valueCell}
              {@render valueCell(row, (v) => update(i, { value: v }))}
            {:else}
              <Input
                mono
                value={row.value}
                aria-label={t('kv.value')}
                placeholder={valuePlaceholder}
                oninput={(e) => update(i, { value: e.currentTarget.value })}
              />
            {/if}
          </span>
          {#if allowFiles}
            <span class="kind">
              <Select
                compact
                label={t('kv.kind')}
                value={row.kind}
                options={[
                  { value: 'text', label: t('kv.kindText') },
                  { value: 'file', label: t('kv.kindFile') },
                ]}
                onchange={(k) => update(i, { kind: k, value: '' })}
              />
            </span>
          {/if}
          {#if showDescription}
            <span class="cell" role="cell">
              <Input
                value={row.description}
                aria-label={t('kv.description')}
                oninput={(e) => update(i, { description: e.currentTarget.value })}
              />
            </span>
          {/if}
          <IconButton icon="close" size={12} label={t('kv.deleteRow')} onclick={() => remove(i)} />
        </div>
      {/each}

      <div class="row ghost" role="row">
        <span class="handle"></span>
        <span class="cell-check"></span>
        <span class="cell" role="cell">
          <Input
            mono
            bind:value={ghostName}
            aria-label={t('kv.newName')}
            placeholder={namePlaceholder ?? t('kv.name')}
            oninput={addFromGhost}
          />
        </span>
        <span class="cell" role="cell">
          <Input
            mono
            bind:value={ghostValue}
            aria-label={t('kv.newValue')}
            placeholder={valuePlaceholder ?? t('kv.value')}
            oninput={addFromGhost}
          />
        </span>
        {#if allowFiles}<span class="kind"></span>{/if}
        {#if showDescription}<span class="cell"></span>{/if}
        <span class="spacer"></span>
      </div>
    </div>
  {/if}
</div>

<style>
  .kv {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }
  .toolbar {
    display: flex;
    justify-content: flex-end;
    gap: var(--space-3);
    font-size: var(--font-size-xs);
  }
  .link {
    color: var(--c-fg-muted);
  }
  .link:hover {
    color: var(--c-fg);
  }
  .link.danger:hover {
    color: var(--c-danger);
  }
  .rows {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: var(--space-1);
  }
  .row.disabled .cell {
    opacity: 0.5;
  }
  .row.drop-target {
    box-shadow: 0 -2px 0 var(--c-accent);
  }
  .handle {
    flex: none;
    width: 14px;
    display: grid;
    place-items: center;
    color: var(--c-fg-subtle);
    fill: currentColor;
    cursor: grab;
    touch-action: none;
  }
  .ghost .handle,
  .inherited .handle {
    cursor: default;
  }
  .cell-check {
    flex: none;
    width: 20px;
    display: grid;
    place-items: center;
  }
  .cell {
    flex: 1;
    min-width: 0;
  }
  .kind {
    flex: none;
    width: 72px;
  }
  .spacer {
    width: var(--control-height);
    flex: none;
  }
  .readonly {
    display: block;
    padding: 0 var(--space-2);
    color: var(--c-fg-subtle);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--font-size-sm);
  }
  .mono {
    font-family: var(--font-mono);
  }
  .inherited-value {
    display: flex;
    align-items: center;
    min-width: 0;
  }
  .inherited-value .readonly {
    flex: 1;
    min-width: 0;
  }
  .row.inherited {
    height: 22px;
  }
  .source {
    flex: none;
    max-width: 40%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--font-size-xs);
    color: var(--c-fg-subtle);
    padding: 0 var(--space-1);
    border-radius: var(--radius-sm);
    background: var(--c-bg-hover);
  }
  .file {
    width: 100%;
    height: var(--control-height);
    padding: 0 var(--space-2);
    border: 1px dashed var(--c-border-strong);
    border-radius: var(--radius-sm);
    color: var(--c-fg-muted);
    text-align: left;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
