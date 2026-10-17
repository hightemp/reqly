<script lang="ts">
  interface Props {
    checked?: boolean;
    disabled?: boolean;
    label: string;
    showLabel?: boolean;
    onchange?: (checked: boolean) => void;
  }

  let {
    checked = $bindable(false),
    disabled = false,
    label,
    showLabel = false,
    onchange,
  }: Props = $props();

  function toggle() {
    if (disabled) return;
    checked = !checked;
    onchange?.(checked);
  }
</script>

<button
  type="button"
  role="switch"
  class="switch"
  aria-checked={checked}
  aria-label={showLabel ? undefined : label}
  {disabled}
  onclick={toggle}
  onkeydown={(e) => {
    if (e.key === ' ' || e.key === 'Enter') {
      e.preventDefault();
      toggle();
    }
  }}
>
  <span class="track" aria-hidden="true"><span class="thumb"></span></span>
  {#if showLabel}<span>{label}</span>{/if}
</button>

<style>
  .switch {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    color: var(--c-fg);
  }
  .track {
    width: 28px;
    height: 16px;
    border-radius: 8px;
    background: var(--c-border-strong);
    position: relative;
    transition: background var(--transition);
  }
  .thumb {
    position: absolute;
    top: 2px;
    left: 2px;
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: var(--c-bg);
    transition: transform var(--transition);
  }
  .switch[aria-checked='true'] .track {
    background: var(--c-accent);
  }
  .switch[aria-checked='true'] .thumb {
    transform: translateX(12px);
    background: var(--c-fg-on-accent);
  }
  .switch:disabled {
    opacity: 0.45;
    pointer-events: none;
  }
</style>
