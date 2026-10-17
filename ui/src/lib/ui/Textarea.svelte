<script lang="ts">
  import type { HTMLTextareaAttributes } from 'svelte/elements';

  interface Props extends HTMLTextareaAttributes {
    value?: string;
    mono?: boolean;
  }

  let { value = $bindable(''), mono = false, class: className, ...rest }: Props = $props();
</script>

<textarea bind:value class="textarea {className ?? ''}" class:mono spellcheck="false" {...rest}
></textarea>

<style>
  .textarea {
    width: 100%;
    min-height: 80px;
    padding: var(--space-2);
    border-radius: var(--radius-sm);
    border: 1px solid var(--c-border);
    background: var(--c-bg-sunken);
    color: var(--c-fg);
    outline: none;
    resize: vertical;
  }
  .textarea:focus {
    border-color: var(--c-accent);
  }
  .mono {
    font-family: var(--font-mono);
    font-size: var(--font-size-sm);
  }
</style>
