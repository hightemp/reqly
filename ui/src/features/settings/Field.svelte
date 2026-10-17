<!-- One settings row: label (+ hint) on the left, control on the right. -->
<script lang="ts">
  import type { Snippet } from 'svelte';

  let { label, hint, children }: { label: string; hint?: string; children: Snippet } = $props();
</script>

<div class="field">
  <div class="text">
    <span class="label">{label}</span>
    {#if hint}<span class="hint">{hint}</span>{/if}
  </div>
  <div class="control">{@render children()}</div>
</div>

<style>
  .field {
    display: grid;
    grid-template-columns: minmax(180px, 1fr) minmax(200px, 1.2fr);
    gap: var(--space-4);
    align-items: center;
    padding: var(--space-2) 0;
    border-bottom: 1px solid var(--c-border);
  }
  .text {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .hint {
    font-size: var(--font-size-xs);
    color: var(--c-fg-subtle);
  }
  .control {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-width: 0;
  }
</style>
