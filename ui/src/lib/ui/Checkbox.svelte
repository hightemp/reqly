<!-- Custom checkbox: a button with role="checkbox"; `label` names it for assistive tech. -->
<script lang="ts">
  interface Props {
    checked?: boolean;
    disabled?: boolean;
    label: string;
    /** Show the label next to the box (otherwise it is only accessible). */
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
  role="checkbox"
  class="checkbox"
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
  <span class="box" aria-hidden="true">
    {#if checked}
      <svg viewBox="0 0 16 16" width="12" height="12"
        ><path
          d="M3.5 8.5l3 3 6-7"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
        /></svg
      >
    {/if}
  </span>
  {#if showLabel}<span class="label">{label}</span>{/if}
</button>

<style>
  .checkbox {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    color: var(--c-fg);
    border-radius: var(--radius-sm);
  }
  .box {
    width: 16px;
    height: 16px;
    border-radius: 4px;
    border: 1.5px solid var(--c-border-strong);
    background: var(--c-bg-sunken);
    display: grid;
    place-items: center;
    color: var(--c-fg-on-accent);
    transition:
      background var(--transition),
      border-color var(--transition);
  }
  .checkbox:hover .box {
    border-color: var(--c-accent);
  }
  .checkbox[aria-checked='true'] .box {
    background: var(--c-accent);
    border-color: var(--c-accent);
  }
  .checkbox:disabled {
    opacity: 0.45;
    pointer-events: none;
  }
</style>
