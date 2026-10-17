<script lang="ts">
  import { t } from '$lib/i18n';
  import { renderMarkdown } from '$lib/markdown';
  import Tabs from '$lib/ui/Tabs.svelte';
  import Textarea from '$lib/ui/Textarea.svelte';

  let { text, onchange }: { text: string; onchange: (text: string) => void } = $props();
  // Start in preview when there is something to read.
  // svelte-ignore state_referenced_locally
  let mode = $state<'edit' | 'preview'>(text ? 'preview' : 'edit');
</script>

<div class="docs">
  <Tabs
    label={t('docs.label')}
    idPrefix="docs"
    bind:value={mode}
    tabs={[
      { id: 'edit', label: t('docs.edit') },
      { id: 'preview', label: t('docs.preview') },
    ]}
  />
  {#if mode === 'edit'}
    <Textarea
      class="area"
      value={text}
      placeholder={t('docs.placeholder')}
      aria-label={t('docs.label')}
      oninput={(e) => onchange(e.currentTarget.value)}
    />
  {:else}
    <!-- Markdown is escaped before rendering; only a safe subset is produced. -->
    <div class="preview selectable">
      {#if text}
        <!-- eslint-disable-next-line svelte/no-at-html-tags -- renderMarkdown escapes input -->
        {@html renderMarkdown(text)}
      {:else}
        <p class="empty">{t('docs.empty')}</p>
      {/if}
    </div>
  {/if}
</div>

<style>
  .docs {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    height: 100%;
  }
  .docs :global(.area) {
    flex: 1;
    min-height: 200px;
    resize: none;
  }
  .preview {
    line-height: 1.6;
    overflow: auto;
  }
  .preview :global(h1),
  .preview :global(h2),
  .preview :global(h3) {
    font-weight: 600;
    margin: var(--space-3) 0 var(--space-2);
  }
  .preview :global(h1) {
    font-size: 18px;
  }
  .preview :global(h2) {
    font-size: 15px;
  }
  .preview :global(p) {
    margin-bottom: var(--space-2);
  }
  .preview :global(code) {
    font-family: var(--font-mono);
    font-size: var(--font-size-sm);
    background: var(--c-bg-sunken);
    padding: 1px 4px;
    border-radius: 3px;
  }
  .preview :global(pre) {
    background: var(--c-bg-sunken);
    padding: var(--space-2);
    border-radius: var(--radius-sm);
    overflow: auto;
  }
  .preview :global(a) {
    color: var(--c-accent);
  }
  .empty {
    color: var(--c-fg-subtle);
  }
</style>
