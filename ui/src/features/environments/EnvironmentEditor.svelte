<!--
  Manage environments: global ones of the workspace, the collection's base
  environment and its sub-environments. Edits save automatically.
-->
<script lang="ts" module>
  /** Preset environment colors (CSS values resolved from theme tokens). */
  export const ENV_COLORS = [
    '#e5484d',
    '#f76b15',
    '#ffc53d',
    '#30a46c',
    '#12a594',
    '#0090ff',
    '#8e4ec6',
    '#d6409f',
  ];
</script>

<script lang="ts">
  import type { Environment } from '$lib/api/generated/Environment';
  import { errorMessage } from '$lib/errors';
  import { t } from '$lib/i18n';
  import type { Services } from '$lib/services.svelte';
  import Button from '$lib/ui/Button.svelte';
  import IconButton from '$lib/ui/IconButton.svelte';
  import Input from '$lib/ui/Input.svelte';
  import { dialogs } from '$lib/ui/overlay/dialogs.svelte';
  import Modal from '$lib/ui/overlay/Modal.svelte';
  import { toasts } from '$lib/ui/overlay/toasts.svelte';
  import VariablesEditor from './VariablesEditor.svelte';

  let { services, onclose }: { services: Services; onclose: () => void } = $props();
  const envs = $derived(services.environments);

  let selectedId = $state<string | null>(null);
  // Prefer the active environment, else the base one.
  $effect(() => {
    if (!selectedId || !envs.find(selectedId)) selectedId = envs.activeId ?? envs.base?.id ?? null;
  });
  const selected = $derived(selectedId ? envs.find(selectedId) : null);

  // Debounced saves keyed by environment.
  const SAVE_DELAY_MS = 400;
  let pending: { id: string; data: Record<string, unknown>; secrets: string[] } | null = null;
  let timer: ReturnType<typeof setTimeout> | null = null;

  async function flush() {
    if (timer) clearTimeout(timer);
    timer = null;
    const p = pending;
    pending = null;
    if (!p) return;
    try {
      await envs.update(p.id, { data: p.data, secrets: p.secrets });
    } catch (e) {
      toasts.error(errorMessage(e));
    }
  }

  function onData(env: Environment, data: Record<string, unknown>, secrets: string[]) {
    if (pending && pending.id !== env.id) void flush();
    pending = { id: env.id, data, secrets };
    if (timer) clearTimeout(timer);
    timer = setTimeout(() => void flush(), SAVE_DELAY_MS);
  }

  async function select(id: string) {
    await flush();
    selectedId = id;
  }

  async function close() {
    await flush();
    onclose();
  }

  async function create(kind: 'sub' | 'global') {
    const name = await dialogs.prompt({
      title: kind === 'sub' ? t('environments.newSub') : t('environments.newGlobal'),
      label: t('actions.name'),
      value: '',
      validate: (v) => (v.trim() ? null : t('actions.nameRequired')),
    });
    if (!name?.trim()) return;
    await flush();
    const env = await envs.create(kind, name.trim());
    if (env) selectedId = env.id;
  }

  async function rename(env: Environment, name: string) {
    if (name.trim() && name !== env.name) await envs.update(env.id, { name: name.trim() });
  }

  async function remove(env: Environment) {
    const ok = await dialogs.confirm({
      title: t('environments.deleteTitle', { name: env.name }),
      confirmLabel: t('tree.delete'),
      danger: true,
    });
    if (!ok) return;
    await envs.remove(env.id);
    selectedId = null;
  }

  async function duplicate(env: Environment) {
    await flush();
    selectedId = (await envs.duplicate(env.id)).id;
  }
</script>

{#snippet item(env: Environment, label: string)}
  <button
    type="button"
    role="option"
    class="item"
    aria-selected={env.id === selectedId}
    onclick={() => void select(env.id)}
  >
    <span class="dot" style:background={env.color ?? 'transparent'}></span>
    <span class="item-name">{label}</span>
    {#if env.id === envs.activeId || env.id === envs.activeGlobalId}
      <span class="badge">{t('environments.active')}</span>
    {/if}
  </button>
{/snippet}

<Modal title={t('environments.title')} width="920px" onclose={() => void close()}>
  <div class="layout">
    <nav class="list" aria-label={t('environments.title')}>
      <div class="group">
        <div class="heading">
          <span>{t('environments.collection')}</span>
          <IconButton
            icon="plus"
            size={14}
            label={t('environments.newSub')}
            onclick={() => void create('sub')}
          />
        </div>
        <div role="listbox" aria-label={t('environments.collection')}>
          {#if envs.base}{@render item(envs.base, t('environments.base'))}{/if}
          {#each envs.subs as env (env.id)}
            <div class="nested">{@render item(env, env.name)}</div>
          {/each}
        </div>
      </div>
      <div class="group">
        <div class="heading">
          <span>{t('environments.global')}</span>
          <IconButton
            icon="plus"
            size={14}
            label={t('environments.newGlobal')}
            onclick={() => void create('global')}
          />
        </div>
        <div role="listbox" aria-label={t('environments.global')}>
          {#each envs.global as env (env.id)}
            {@render item(env, env.name)}
          {:else}
            <p class="empty">{t('environments.noGlobal')}</p>
          {/each}
        </div>
      </div>
    </nav>

    <section class="detail">
      {#if selected}
        {#key selected.id}
          <header class="detail-head">
            {#if selected.kind === 'base'}
              <h3>{t('environments.base')}</h3>
            {:else}
              <div class="name-input">
                <Input
                  value={selected.name}
                  aria-label={t('actions.name')}
                  onchange={(e) => void rename(selected, e.currentTarget.value)}
                />
              </div>
            {/if}
            <div class="colors" role="radiogroup" aria-label={t('environments.color')}>
              <button
                type="button"
                role="radio"
                class="swatch none"
                aria-checked={!selected.color}
                aria-label={t('environments.noColor')}
                onclick={() => void envs.update(selected.id, { color: '' })}
              ></button>
              {#each ENV_COLORS as color (color)}
                <button
                  type="button"
                  role="radio"
                  class="swatch"
                  style:background={color}
                  aria-checked={selected.color === color}
                  aria-label={color}
                  onclick={() => void envs.update(selected.id, { color })}
                ></button>
              {/each}
            </div>
            {#if selected.kind !== 'base'}
              <span class="actions">
                {#if selected.kind === 'sub'}
                  <Button
                    onclick={() =>
                      void envs.setActive(envs.activeId === selected.id ? null : selected.id)}
                  >
                    {envs.activeId === selected.id
                      ? t('environments.deactivate')
                      : t('environments.activate')}
                  </Button>
                {:else}
                  <Button
                    onclick={() =>
                      void envs.setActiveGlobal(
                        envs.activeGlobalId === selected.id ? null : selected.id,
                      )}
                  >
                    {envs.activeGlobalId === selected.id
                      ? t('environments.deactivate')
                      : t('environments.activate')}
                  </Button>
                {/if}
                <IconButton
                  icon="copy"
                  label={t('environments.duplicate')}
                  onclick={() => void duplicate(selected)}
                />
                <IconButton
                  icon="trash"
                  variant="danger"
                  label={t('tree.delete')}
                  onclick={() => void remove(selected)}
                />
              </span>
            {/if}
          </header>
          {#if selected.kind === 'base'}
            <p class="note">{t('environments.baseHint')}</p>
          {/if}
          <VariablesEditor
            label={t('environments.variables')}
            data={selected.data}
            secrets={selected.secrets}
            allowSecrets
            onchange={(data, secrets) => onData(selected, data, secrets)}
          />
        {/key}
      {/if}
    </section>
  </div>
</Modal>

<style>
  .layout {
    display: flex;
    gap: var(--space-3);
    height: min(560px, 70vh);
  }
  .list {
    width: 220px;
    flex: none;
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    overflow: auto;
    border-right: 1px solid var(--c-border);
    padding-right: var(--space-2);
  }
  .group {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .heading {
    display: flex;
    align-items: center;
    justify-content: space-between;
    font-size: var(--font-size-xs);
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--c-fg-subtle);
  }
  .item {
    width: 100%;
    display: flex;
    align-items: center;
    gap: var(--space-2);
    height: var(--row-height, 28px);
    padding: 0 var(--space-2);
    border-radius: var(--radius-sm);
    text-align: left;
    color: var(--c-fg);
  }
  .item:hover {
    background: var(--c-bg-hover);
  }
  .item[aria-selected='true'] {
    background: var(--c-bg-selected);
  }
  .nested .item {
    padding-left: var(--space-5, 20px);
  }
  .item-name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .dot {
    width: 8px;
    height: 8px;
    flex: none;
    border-radius: 50%;
    border: 1px solid var(--c-border-strong);
  }
  .badge {
    font-size: var(--font-size-xs);
    color: var(--c-accent);
  }
  .empty,
  .note {
    margin: 0;
    padding: var(--space-1) var(--space-2);
    color: var(--c-fg-subtle);
    font-size: var(--font-size-sm);
  }
  .detail {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }
  .detail-head {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    flex-wrap: wrap;
  }
  .detail-head h3 {
    margin: 0;
    font-size: var(--font-size);
  }
  .name-input {
    width: 220px;
  }
  .colors {
    display: flex;
    gap: 4px;
  }
  .swatch {
    width: 16px;
    height: 16px;
    border-radius: 50%;
    border: 1px solid var(--c-border-strong);
  }
  .swatch[aria-checked='true'] {
    outline: 2px solid var(--c-focus-ring);
    outline-offset: 1px;
  }
  .swatch.none {
    background: linear-gradient(
      135deg,
      transparent 45%,
      var(--c-danger) 45%,
      var(--c-danger) 55%,
      transparent 55%
    );
  }
  .actions {
    margin-left: auto;
    display: flex;
    align-items: center;
    gap: var(--space-1);
  }
</style>
