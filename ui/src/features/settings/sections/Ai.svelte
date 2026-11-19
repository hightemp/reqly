<!-- AI chat settings (ADR-015): on/off, providers, the default model and
     the tools the chat may use. -->
<script lang="ts">
  import type { AiPreset } from '$lib/api/generated/AiPreset';
  import type { McpGroup } from '$lib/api/generated/McpGroup';
  import { errorMessage } from '$lib/errors';
  import { t } from '$lib/i18n';
  import type { AiStore } from '$lib/stores/ai.svelte';
  import Button from '$lib/ui/Button.svelte';
  import Checkbox from '$lib/ui/Checkbox.svelte';
  import IconButton from '$lib/ui/IconButton.svelte';
  import Switch from '$lib/ui/Switch.svelte';
  import Combobox from '$lib/ui/overlay/Combobox.svelte';
  import Select from '$lib/ui/overlay/Select.svelte';
  import { menus } from '$lib/ui/overlay/menus.svelte';
  import { toasts } from '$lib/ui/overlay/toasts.svelte';
  import HelpButton from '../../help/HelpButton.svelte';
  import Field from '../Field.svelte';
  import AiProvider from './AiProvider.svelte';

  let { ai }: { ai: AiStore } = $props();

  let open = $state<McpGroup[]>([]);
  let loadingModels = $state(false);
  // i18n-dynamic: ai.preset.
  // i18n-dynamic: mcp.groups.

  /** A provider picked here before choosing its model (saved with the model). */
  let picked = $state<string | null>(null);
  const providerId = $derived(
    (picked && ai.providers.some((p) => p.id === picked) ? picked : null) ??
      ai.defaultModel?.providerId ??
      ai.providers[0]?.id ??
      '',
  );
  const model = $derived(ai.defaultModel?.providerId === providerId ? ai.defaultModel.model : '');
  const models = $derived(ai.models[providerId] ?? []);

  async function run(action: () => Promise<unknown>) {
    try {
      await action();
    } catch (e) {
      toasts.error(errorMessage(e));
    }
  }

  function addMenu(e: MouseEvent) {
    menus.openBelow(
      e.currentTarget as HTMLElement,
      ai.presets.map((p) => ({
        id: p.preset,
        label: t(`ai.preset.${p.preset as AiPreset}`),
        onSelect: () => void run(() => ai.addProvider(p.preset)),
      })),
    );
  }

  async function loadModels() {
    if (!providerId) return;
    loadingModels = true;
    await run(() => ai.loadModels(providerId));
    loadingModels = false;
  }

  function toggleOpen(group: McpGroup) {
    open = open.includes(group) ? open.filter((g) => g !== group) : [...open, group];
  }
</script>

<div class="intro">
  <p>{t('ai.intro')}</p>
  <HelpButton page="ai" />
</div>

{#if !ai.canEdit}
  <p class="note" role="note">{t('ai.viewOnly')}</p>
{/if}

<fieldset disabled={!ai.canEdit}>
  <Field label={t('ai.enable')} hint={t('ai.enableHint')}>
    <Switch
      label={t('ai.enable')}
      checked={ai.enabled}
      onchange={(on) => void run(() => ai.setEnabled(on))}
    />
  </Field>
  <p class="privacy">{t('ai.privacy')}</p>

  <section class="block">
    <div class="block-head">
      <div>
        <h3>{t('ai.providers')}</h3>
        <span class="hint">{t('ai.providersHint')}</span>
      </div>
      {#if ai.canEdit}
        <Button size="sm" onclick={addMenu}>{t('ai.addProvider')}</Button>
      {/if}
    </div>
    <div class="providers">
      {#each ai.providers as provider (provider.id)}
        {#if ai.canEdit}
          <AiProvider {ai} {provider} />
        {:else}
          <div class="readonly">{provider.name} <span class="hint">{provider.baseUrl}</span></div>
        {/if}
      {/each}
    </div>
  </section>

  <section class="block">
    <h3>{t('ai.defaultModel')}</h3>
    <span class="hint">{t('ai.defaultModelHint')}</span>
    {#if ai.providers.length === 0}
      <p class="muted">{t('ai.noProviders')}</p>
    {:else}
      <div class="model">
        <Select
          label={t('ai.provider')}
          value={providerId}
          options={ai.providers.map((p) => ({ value: p.id, label: p.name }))}
          onchange={(id) => {
            picked = id;
            if (!ai.models[id]) void ai.loadModels(id).catch(() => {});
          }}
        />
        <Combobox
          label={t('ai.model')}
          value={model}
          options={models}
          allowCustom
          mono
          placeholder={t('ai.modelPlaceholder')}
          onchange={(m) => {
            if (m.trim()) void run(() => ai.setDefaultModel(providerId, m.trim()));
          }}
        />
        <Button size="sm" variant="ghost" loading={loadingModels} onclick={() => void loadModels()}
          >{t('ai.loadModels')}</Button
        >
      </div>
    {/if}
  </section>

  <section class="block">
    <h3>{t('ai.tools')}</h3>
    <span class="hint">{t('ai.toolsHint')}</span>
    <ul class="groups">
      {#each ai.groups as g (g.group)}
        {@const label = t(`mcp.groups.${g.group}`)}
        <li>
          <div class="group">
            <IconButton
              icon={open.includes(g.group) ? 'arrow-down' : 'arrow-up'}
              size={12}
              label={t('mcp.showTools', { group: label })}
              pressed={open.includes(g.group)}
              onclick={() => toggleOpen(g.group)}
            />
            <Checkbox
              {label}
              showLabel
              checked={g.enabled === g.total}
              onchange={(on) => void run(() => ai.setGroup(g.group, on))}
            />
            <span class="count">{t('mcp.groupCount', { enabled: g.enabled, total: g.total })}</span>
            {#if g.group !== 'read'}
              <span class="auto">
                <Checkbox
                  label={t('ai.autoApprove')}
                  showLabel
                  checked={g.autoApprove}
                  onchange={(on) => void run(() => ai.setAutoApprove(g.group, on))}
                />
              </span>
            {/if}
          </div>
          {#if open.includes(g.group)}
            <ul class="tools">
              {#each g.tools as tool (tool.name)}
                <li>
                  <Checkbox
                    label={tool.name}
                    showLabel
                    checked={tool.enabled}
                    onchange={(on) => void run(() => ai.setTool(tool.name, on))}
                  />
                  {#if tool.needsConfirmation}<span class="asks">{t('ai.asks')}</span>{/if}
                </li>
              {/each}
            </ul>
          {/if}
        </li>
      {/each}
    </ul>
  </section>
</fieldset>

<style>
  fieldset {
    border: 0;
    margin: 0;
    padding: 0;
    min-width: 0;
  }
  .intro {
    display: flex;
    align-items: flex-start;
    gap: var(--space-2);
    margin-bottom: var(--space-2);
    color: var(--c-fg-muted);
  }
  .intro p {
    margin: 0;
    flex: 1;
  }
  .note,
  .privacy {
    margin: var(--space-2) 0;
    padding: var(--space-2);
    border-radius: var(--radius-sm);
    background: var(--c-bg-sunken);
    font-size: var(--font-size-sm);
  }
  .privacy {
    color: var(--c-fg-muted);
  }
  .block {
    padding: var(--space-3) 0;
    border-bottom: 1px solid var(--c-border);
    display: flex;
    flex-direction: column;
    gap: var(--space-1, 4px);
  }
  .block-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
    margin-bottom: var(--space-2);
  }
  h3 {
    margin: 0;
    font-size: var(--font-size);
  }
  .hint,
  .muted,
  .count,
  .asks {
    color: var(--c-fg-subtle);
    font-size: var(--font-size-xs);
  }
  .providers {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }
  .readonly {
    padding: var(--space-1, 4px) 0;
  }
  .model {
    display: flex;
    gap: var(--space-2);
    align-items: center;
    flex-wrap: wrap;
    margin-top: var(--space-2);
  }
  .groups,
  .tools {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .group {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: 4px 0;
  }
  .auto {
    margin-left: auto;
  }
  .tools {
    padding-left: 44px;
    font-family: var(--font-mono);
  }
  .tools li {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: 2px 0;
  }
</style>
