<script lang="ts">
  import type { HTMLInputAttributes } from 'svelte/elements';

  interface Props extends HTMLInputAttributes {
    value?: string;
    invalid?: boolean;
    mono?: boolean;
    inputRef?: HTMLInputElement | null;
  }

  let {
    value = $bindable(''),
    invalid = false,
    mono = false,
    inputRef = $bindable(null),
    class: className,
    ...rest
  }: Props = $props();
</script>

<input
  bind:this={inputRef}
  bind:value
  class="input {className ?? ''}"
  class:mono
  aria-invalid={invalid || undefined}
  spellcheck="false"
  autocomplete="off"
  {...rest}
/>

<style>
  .input {
    width: 100%;
    height: var(--control-height);
    padding: 0 var(--space-2);
    border-radius: var(--radius-sm);
    border: 1px solid var(--c-border);
    background: var(--c-bg-sunken);
    color: var(--c-fg);
    outline: none;
    transition: border-color var(--transition);
  }
  .input::placeholder {
    color: var(--c-fg-subtle);
  }
  .input:hover {
    border-color: var(--c-border-strong);
  }
  .input:focus {
    border-color: var(--c-accent);
  }
  .input[aria-invalid='true'] {
    border-color: var(--c-danger);
  }
  .input:disabled {
    opacity: 0.5;
  }
  .mono {
    font-family: var(--font-mono);
    font-size: var(--font-size-sm);
  }
</style>
