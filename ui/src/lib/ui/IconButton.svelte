<!-- Square button with an icon. `label` is required for accessibility. -->
<script lang="ts">
  import type { HTMLButtonAttributes } from 'svelte/elements';
  import Icon, { type IconName } from './Icon.svelte';

  interface Props extends Omit<HTMLButtonAttributes, 'children'> {
    icon: IconName;
    label: string;
    size?: number;
    pressed?: boolean;
    variant?: 'ghost' | 'danger';
  }

  let {
    icon,
    label,
    size = 16,
    pressed,
    variant = 'ghost',
    class: className,
    ...rest
  }: Props = $props();
</script>

<button
  type="button"
  class="icon-button {variant} {className ?? ''}"
  aria-label={label}
  aria-pressed={pressed}
  {...rest}
>
  <Icon name={icon} {size} />
</button>

<style>
  .icon-button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: var(--control-height);
    height: var(--control-height);
    border-radius: var(--radius-sm);
    color: var(--c-fg-muted);
    transition:
      background var(--transition),
      color var(--transition);
  }
  .icon-button:hover {
    background: var(--c-bg-hover);
    color: var(--c-fg);
  }
  .icon-button:active,
  .icon-button[aria-pressed='true'] {
    background: var(--c-bg-active);
    color: var(--c-fg);
  }
  .icon-button.danger:hover {
    background: var(--c-danger);
    color: var(--c-fg-on-accent);
  }
  .icon-button:disabled {
    opacity: 0.4;
    pointer-events: none;
  }
</style>
