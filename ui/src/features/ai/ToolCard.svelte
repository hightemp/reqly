<!-- A tool call of the model: name, state, arguments and result; the
     Allow / Deny buttons while it waits for the user. -->
<script lang="ts">
  import type { AiChatMessage } from '$lib/api/generated/AiChatMessage';
  import type { AiDecision } from '$lib/api/generated/AiDecision';
  import type { AiToolCall } from '$lib/api/generated/AiToolCall';
  import { errorMessage } from '$lib/errors';
  import { t } from '$lib/i18n';
  import Button from '$lib/ui/Button.svelte';
  import Spinner from '$lib/ui/Spinner.svelte';

  interface Props {
    message: AiChatMessage;
    call?: AiToolCall;
    waiting: boolean;
    onconfirm: (decision: AiDecision) => void;
  }

  let { message, call, waiting, onconfirm }: Props = $props();
  // i18n-dynamic: ai.toolState.

  const MAX_SHOWN = 6000;

  /** Pretty JSON when it parses, the text otherwise; shortened. */
  function pretty(text: string | undefined): string {
    if (!text) return '';
    let out = text;
    try {
      out = JSON.stringify(JSON.parse(text), null, 2);
    } catch {
      // Not JSON: as it is.
    }
    return out.length > MAX_SHOWN ? `${out.slice(0, MAX_SHOWN)}\n…` : out;
  }

  const name = $derived(call?.name ?? message.toolName ?? '');
  const state = $derived(waiting ? 'waiting' : message.status);
</script>

<div class="tool {state}" role="group" aria-label={name}>
  {#if waiting}
    <div class="ask">
      <span class="title">{t('ai.confirmTitle', { tool: name })}</span>
      <pre class="mono">{pretty(call?.arguments)}</pre>
      <div class="actions">
        <Button size="sm" variant="primary" onclick={() => onconfirm('allow')}
          >{t('ai.allow')}</Button
        >
        <Button size="sm" onclick={() => onconfirm('allowAlways')}>{t('ai.allowAlways')}</Button>
        <Button size="sm" variant="ghost" onclick={() => onconfirm('deny')}>{t('ai.deny')}</Button>
      </div>
    </div>
  {:else}
    <details>
      <summary>
        {#if state === 'pending'}<Spinner size={12} />{/if}
        <span class="name mono">{name}</span>
        <span class="state">{t(`ai.toolState.${state}`)}</span>
      </summary>
      <div class="body">
        <span class="label">{t('ai.arguments')}</span>
        <pre class="mono">{pretty(call?.arguments)}</pre>
        {#if message.error}
          <span class="label">{t('ai.result')}</span>
          <p class="error">{errorMessage(message.error)}</p>
        {:else if message.content && state !== 'denied' && state !== 'cancelled'}
          <span class="label">{t('ai.result')}</span>
          <pre class="mono">{pretty(message.content)}</pre>
        {/if}
      </div>
    </details>
  {/if}
</div>

<style>
  .tool {
    border: 1px solid var(--c-border);
    border-radius: var(--radius-sm);
    font-size: var(--font-size-sm);
    background: var(--c-bg-sunken);
  }
  .tool.waiting {
    border-color: var(--c-accent);
  }
  summary {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: 4px var(--space-2);
    cursor: pointer;
    list-style: none;
  }
  summary::-webkit-details-marker {
    display: none;
  }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .state {
    margin-left: auto;
    color: var(--c-fg-subtle);
    font-size: var(--font-size-xs);
  }
  .error .state,
  .tool.error .state {
    color: var(--c-danger);
  }
  .tool.done .state {
    color: var(--c-success);
  }
  .body,
  .ask {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 4px var(--space-2) var(--space-2);
  }
  .title {
    font-weight: 600;
  }
  .label {
    color: var(--c-fg-subtle);
    font-size: var(--font-size-xs);
  }
  pre {
    margin: 0;
    max-height: 240px;
    overflow: auto;
    white-space: pre-wrap;
    word-break: break-all;
    font-size: var(--font-size-xs);
  }
  .error {
    margin: 0;
    color: var(--c-danger);
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
  }
</style>
