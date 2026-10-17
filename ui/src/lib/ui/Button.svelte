<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { HTMLButtonAttributes } from 'svelte/elements';
  import Spinner from './Spinner.svelte';

  interface Props extends HTMLButtonAttributes {
    variant?: 'default' | 'primary' | 'ghost' | 'danger';
    size?: 'sm' | 'md';
    loading?: boolean;
    children?: Snippet;
  }

  let {
    variant = 'default',
    size = 'md',
    loading = false,
    disabled,
    children,
    class: className,
    onclick,
    ...rest
  }: Props = $props();
</script>

<button
  type="button"
  class="btn {variant} {size} {className ?? ''}"
  {disabled}
  aria-busy={loading || undefined}
  onclick={(e) => {
    if (!loading) onclick?.(e);
  }}
  {...rest}
>
  {#if loading}<Spinner size={12} />{/if}
  {@render children?.()}
</button>

<style>
  .btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: var(--space-2);
    height: var(--control-height);
    padding: 0 var(--space-3);
    border-radius: var(--radius-sm);
    border: 1px solid var(--c-border-strong);
    background: var(--c-bg-elevated);
    color: var(--c-fg);
    font-weight: 500;
    white-space: nowrap;
    transition:
      background var(--transition),
      border-color var(--transition),
      color var(--transition);
  }
  .btn:hover {
    background: var(--c-bg-hover);
  }
  .btn:active {
    background: var(--c-bg-active);
  }
  .btn.sm {
    height: calc(var(--control-height) - 4px);
    padding: 0 var(--space-2);
    font-size: var(--font-size-sm);
  }
  .primary {
    background: var(--c-accent);
    border-color: var(--c-accent);
    color: var(--c-fg-on-accent);
  }
  .primary:hover {
    background: var(--c-accent-hover);
  }
  .ghost {
    background: transparent;
    border-color: transparent;
    color: var(--c-fg-muted);
  }
  .ghost:hover {
    color: var(--c-fg);
  }
  .danger {
    background: var(--c-danger);
    border-color: var(--c-danger);
    color: var(--c-fg-on-accent);
  }
  .danger:hover {
    filter: brightness(1.08);
  }
  .btn:disabled {
    opacity: 0.45;
    pointer-events: none;
  }
  .btn[aria-busy='true'] {
    cursor: progress;
  }
</style>
