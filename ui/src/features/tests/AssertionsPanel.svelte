<!-- No-code assertions of a request (the Tests tab). -->
<script lang="ts">
  import type { Assertion } from '$lib/api/generated/Assertion';
  import { t } from '$lib/i18n';
  import Button from '$lib/ui/Button.svelte';
  import Checkbox from '$lib/ui/Checkbox.svelte';
  import CodeEditor from '$lib/ui/code/CodeEditor.svelte';
  import TemplateInput from '$lib/ui/code/TemplateInput.svelte';
  import IconButton from '$lib/ui/IconButton.svelte';
  import Select from '$lib/ui/overlay/Select.svelte';
  import {
    OPERATORS,
    SOURCES,
    needsProperty,
    needsValue,
    newAssertion,
    operatorLabel,
    sourceLabel,
  } from './assertions';

  interface Props {
    assertions: Assertion[];
    nodeId?: string | null;
    onchange: (assertions: Assertion[]) => void;
  }

  let { assertions, nodeId = null, onchange }: Props = $props();

  const sources = $derived(SOURCES.map((value) => ({ value, label: sourceLabel(value) })));
  const operators = $derived(OPERATORS.map((value) => ({ value, label: operatorLabel(value) })));

  function update(index: number, patch: Partial<Assertion>) {
    onchange(assertions.map((a, i) => (i === index ? { ...a, ...patch } : a)));
  }

  // i18n-dynamic: assertions.placeholders.
  const placeholder = (a: Assertion) =>
    needsProperty(a.source)
      ? t(`assertions.placeholders.${a.source}` as 'assertions.placeholders.header')
      : '';
</script>

<div class="assertions">
  {#if assertions.length === 0}
    <p class="hint">{t('assertions.empty')}</p>
  {:else}
    <ul aria-label={t('assertions.list')}>
      {#each assertions as a, i (a.id)}
        <li class:disabled={!a.enabled}>
          <div class="row">
            <Checkbox
              label={t('assertions.enabled')}
              checked={a.enabled}
              onchange={(enabled) => update(i, { enabled })}
            />
            <div class="source">
              <Select
                label={t('assertions.source')}
                compact
                value={a.source}
                options={sources}
                onchange={(source) => update(i, { source })}
              />
            </div>
            {#if needsProperty(a.source)}
              <div class="property">
                <TemplateInput
                  label={t('assertions.property')}
                  value={a.property}
                  placeholder={placeholder(a)}
                  {nodeId}
                  onchange={(property) => update(i, { property })}
                />
              </div>
            {/if}
            <div class="operator">
              <Select
                label={t('assertions.operator')}
                compact
                value={a.operator}
                options={operators}
                onchange={(operator) => update(i, { operator })}
              />
            </div>
            {#if needsValue(a.operator) && a.operator !== 'matchesSchema'}
              <div class="value">
                <TemplateInput
                  label={t('assertions.value')}
                  value={a.value}
                  {nodeId}
                  onchange={(value) => update(i, { value })}
                />
              </div>
            {/if}
            <IconButton
              icon="trash"
              label={t('assertions.remove')}
              onclick={() => onchange(assertions.filter((_, j) => j !== i))}
            />
          </div>
          {#if a.operator === 'matchesSchema'}
            <div class="schema">
              <CodeEditor
                label={t('assertions.schema')}
                language="json"
                templates
                {nodeId}
                value={a.value}
                onchange={(value) => update(i, { value })}
              />
            </div>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
  <div>
    <Button size="sm" onclick={() => onchange([...assertions, newAssertion()])}>
      {t('assertions.add')}
    </Button>
  </div>
</div>

<style>
  .assertions {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }
  .hint {
    color: var(--c-fg-muted);
  }
  ul {
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }
  .row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex-wrap: wrap;
  }
  .disabled .row > :not(:first-child) {
    opacity: 0.6;
  }
  .source,
  .operator {
    flex: 0 0 auto;
    min-width: 120px;
  }
  .property,
  .value {
    flex: 1 1 140px;
    min-width: 0;
  }
  .schema {
    margin-top: var(--space-2);
    height: 160px;
    border: 1px solid var(--c-border);
    border-radius: var(--radius-sm);
    overflow: hidden;
  }
</style>
