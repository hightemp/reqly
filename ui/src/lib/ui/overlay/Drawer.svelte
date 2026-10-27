<!--
  Panel sliding in from the left (navigation) or the bottom (sheet), over a
  scrim that closes it. Escape closes it too; focus stays inside.
-->
<script lang="ts">
  import { onMount, type Snippet } from 'svelte';
  import { focusable, trapTab } from './focus';
  import { portal } from './portal';

  interface Props {
    label: string;
    /** Accessible name of the scrim, which closes the drawer. */
    closeLabel: string;
    side?: 'left' | 'bottom';
    onclose: () => void;
    children: Snippet;
  }

  let { label, closeLabel, side = 'left', onclose, children }: Props = $props();
  let panel: HTMLDivElement | null = $state(null);

  onMount(() => {
    const previous = document.activeElement as HTMLElement | null;
    queueMicrotask(() => (panel ? (focusable(panel)[0] ?? panel) : null)?.focus());
    return () => previous?.focus?.();
  });

  function onKeyDown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      e.stopPropagation();
      onclose();
      return;
    }
    if (panel) trapTab(panel, e);
  }
</script>

<div use:portal class="root {side}">
  <button type="button" class="scrim" aria-label={closeLabel} tabindex="-1" onclick={onclose}
  ></button>
  <div
    bind:this={panel}
    class="panel"
    role="dialog"
    aria-modal="true"
    aria-label={label}
    tabindex="-1"
    onkeydown={onKeyDown}
  >
    {@render children()}
  </div>
</div>

<style>
  .root {
    position: fixed;
    inset: 0;
    z-index: 800;
    display: flex;
  }
  .root.bottom {
    flex-direction: column;
    justify-content: flex-end;
  }
  .scrim {
    position: absolute;
    inset: 0;
    background: var(--c-overlay);
    cursor: default;
  }
  .panel {
    position: relative;
    display: flex;
    flex-direction: column;
    background: var(--c-bg-elevated);
    box-shadow: 0 10px 40px var(--c-shadow);
    outline: none;
  }
  .left .panel {
    width: min(320px, 85vw);
    height: 100%;
    padding-top: var(--safe-top, 0px);
    padding-bottom: var(--safe-bottom, 0px);
    border-right: 1px solid var(--c-border-strong);
    animation: slide-right 160ms ease-out;
  }
  .bottom .panel {
    max-height: 70vh;
    padding-bottom: var(--safe-bottom, 0px);
    border-top: 1px solid var(--c-border-strong);
    border-radius: var(--radius-lg) var(--radius-lg) 0 0;
    animation: slide-up 160ms ease-out;
  }
  @keyframes slide-right {
    from {
      transform: translateX(-100%);
    }
  }
  @keyframes slide-up {
    from {
      transform: translateY(100%);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .panel {
      animation: none;
    }
  }
</style>
