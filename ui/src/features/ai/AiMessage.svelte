<!-- One chat message: the user's text with attachments, or the model's
     answer (Markdown, reasoning, state, errors). Tool results are cards. -->
<script lang="ts">
  import type { AiChatMessage } from '$lib/api/generated/AiChatMessage';
  import { errorMessage } from '$lib/errors';
  import { t } from '$lib/i18n';
  import { renderMarkdown } from '$lib/markdown';
  import { highlightToHtml } from '$lib/ui/code/highlight';
  import Spinner from '$lib/ui/Spinner.svelte';
  import { toasts } from '$lib/ui/overlay/toasts.svelte';

  interface Props {
    message: AiChatMessage;
    /** The tool the model is starting to call (while streaming). */
    calling?: string;
  }

  let { message, calling }: Props = $props();

  const html = $derived(
    message.content
      ? renderMarkdown(message.content, { highlight: highlightToHtml, copyLabel: t('ai.copyCode') })
      : '',
  );
  const pending = $derived(message.status === 'pending');

  async function onclick(e: MouseEvent) {
    const button = (e.target as HTMLElement).closest('.md-copy');
    const code = button?.parentElement?.querySelector('pre')?.textContent;
    if (code === undefined || code === null) return;
    await navigator.clipboard.writeText(code);
    toasts.show({ message: t('ai.copied'), kind: 'success' });
  }
</script>

{#if message.role === 'user'}
  <div class="message user">
    {#if message.attachments.length}
      <div class="attachments">
        {#each message.attachments as a (a.id)}
          <span class="chip">{a.label}</span>
        {/each}
      </div>
    {/if}
    <p class="text">{message.content}</p>
  </div>
{:else if message.role === 'assistant'}
  <div class="message assistant" class:failed={message.status === 'error'}>
    {#if message.reasoning}
      <details class="reasoning">
        <summary>{t('ai.thinking')}</summary>
        <p>{message.reasoning}</p>
      </details>
    {/if}
    {#if html}
      <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
      <!-- eslint-disable-next-line svelte/no-at-html-tags -- escaped by renderMarkdown -->
      <div class="md" onclick={(e) => void onclick(e)}>{@html html}</div>
    {/if}
    {#if pending}
      <span class="status">
        <Spinner size={12} />
        {calling ? t('ai.calling', { tool: calling }) : html ? '' : t('ai.thinking')}
      </span>
    {:else if message.status === 'cancelled'}
      <span class="status">{t('ai.stopped')}</span>
    {:else if message.error}
      <p class="error" role="alert">{errorMessage(message.error)}</p>
    {/if}
    {#if message.usage && !pending}
      <span class="usage"
        >{t('ai.tokens', {
          prompt: message.usage.promptTokens,
          completion: message.usage.completionTokens,
        })}</span
      >
    {/if}
  </div>
{/if}

<style>
  .message {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
  }
  .user {
    align-self: flex-end;
    max-width: 90%;
    padding: var(--space-2);
    border-radius: var(--radius-md, 8px);
    background: var(--c-accent-subtle, var(--c-bg-sunken));
  }
  .text {
    margin: 0;
    white-space: pre-wrap;
    word-break: break-word;
  }
  .attachments {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }
  .chip {
    font-size: var(--font-size-xs);
    padding: 1px 6px;
    border-radius: 999px;
    border: 1px solid var(--c-border);
    color: var(--c-fg-muted);
  }
  .reasoning {
    font-size: var(--font-size-sm);
    color: var(--c-fg-subtle);
  }
  .reasoning p {
    margin: 4px 0 0;
    white-space: pre-wrap;
  }
  .status,
  .usage {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--c-fg-subtle);
    font-size: var(--font-size-xs);
  }
  .error {
    margin: 0;
    color: var(--c-danger);
  }
  .md {
    word-break: break-word;
    line-height: 1.5;
  }
  .md :global(p) {
    margin: 0 0 var(--space-2);
  }
  .md :global(p:last-child) {
    margin-bottom: 0;
  }
  .md :global(ul),
  .md :global(ol) {
    margin: 0 0 var(--space-2);
    padding-left: 1.4em;
  }
  .md :global(h1),
  .md :global(h2),
  .md :global(h3),
  .md :global(h4) {
    font-size: var(--font-size);
    margin: var(--space-2) 0 4px;
  }
  .md :global(code) {
    font-family: var(--font-mono);
    font-size: var(--font-size-sm);
  }
  .md :global(:not(pre) > code) {
    padding: 0 3px;
    border-radius: 3px;
    background: var(--c-bg-sunken);
  }
  .md :global(.md-code) {
    position: relative;
    margin: 0 0 var(--space-2);
  }
  .md :global(pre) {
    margin: 0 0 var(--space-2);
    padding: var(--space-2);
    border-radius: var(--radius-sm);
    background: var(--c-bg-sunken);
    overflow-x: auto;
  }
  .md :global(.md-code pre) {
    margin: 0;
  }
  .md :global(.md-copy) {
    position: absolute;
    top: 4px;
    right: 4px;
    width: 22px;
    height: 22px;
    border: 1px solid var(--c-border);
    border-radius: var(--radius-sm);
    background: var(--c-bg);
    cursor: pointer;
    opacity: 0;
  }
  .md :global(.md-copy)::before {
    content: '⧉';
    font-size: 12px;
    color: var(--c-fg-muted);
  }
  .md :global(.md-code:hover .md-copy),
  .md :global(.md-copy:focus-visible) {
    opacity: 1;
  }
  .md :global(table) {
    border-collapse: collapse;
    margin: 0 0 var(--space-2);
    font-size: var(--font-size-sm);
  }
  .md :global(th),
  .md :global(td) {
    border: 1px solid var(--c-border);
    padding: 2px 6px;
  }
  .md :global(.hl-keyword) {
    color: var(--c-syntax-keyword);
  }
  .md :global(.hl-string) {
    color: var(--c-syntax-string);
  }
  .md :global(.hl-number) {
    color: var(--c-syntax-number);
  }
  .md :global(.hl-boolean) {
    color: var(--c-syntax-boolean);
  }
  .md :global(.hl-null) {
    color: var(--c-syntax-null);
  }
  .md :global(.hl-property) {
    color: var(--c-syntax-property);
  }
  .md :global(.hl-comment) {
    color: var(--c-syntax-comment);
    font-style: italic;
  }
  .md :global(.hl-punctuation) {
    color: var(--c-syntax-punctuation);
  }
  .md :global(.hl-variable) {
    color: var(--c-syntax-variable);
  }
  .md :global(.hl-tag) {
    color: var(--c-syntax-tag);
  }
  .md :global(.hl-attribute) {
    color: var(--c-syntax-attribute);
  }
  .md :global(.hl-function) {
    color: var(--c-syntax-function);
  }
</style>
