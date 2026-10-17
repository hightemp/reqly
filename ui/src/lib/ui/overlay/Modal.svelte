<!--
  Modal dialog: backdrop, focus trap, Escape to close, focus restored on close.
-->
<script lang="ts">
  import { onMount, type Snippet } from 'svelte';
  import { trapTab, focusable } from './focus';
  import { portal } from './portal';

  interface Props {
    title: string;
    role?: 'dialog' | 'alertdialog';
    width?: string;
    onclose: () => void;
    /** Element to focus first; defaults to the first focusable element. */
    initialFocus?: () => HTMLElement | null | undefined;
    children: Snippet;
    footer?: Snippet;
    header?: Snippet;
  }

  let {
    title,
    role = 'dialog',
    width = '440px',
    onclose,
    initialFocus,
    children,
    footer,
    header,
  }: Props = $props();
  let panel: HTMLDivElement | null = $state(null);
  const titleId = `modal-title-${Math.random().toString(36).slice(2)}`;

  onMount(() => {
    const previous = document.activeElement as HTMLElement | null;
    queueMicrotask(() =>
      (initialFocus?.() ?? (panel ? focusable(panel)[0] : null) ?? panel)?.focus(),
    );
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

<div
  use:portal
  class="backdrop"
  role="presentation"
  onpointerdown={(e) => e.target === e.currentTarget && onclose()}
>
  <div
    bind:this={panel}
    class="panel"
    {role}
    aria-modal="true"
    aria-labelledby={titleId}
    tabindex="-1"
    style:width
    onkeydown={onKeyDown}
  >
    <header class="head">
      <h2 id={titleId}>{title}</h2>
      {@render header?.()}
    </header>
    <div class="content">{@render children()}</div>
    {#if footer}<footer class="foot">{@render footer()}</footer>{/if}
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 900;
    background: var(--c-overlay);
    display: grid;
    place-items: center;
    padding: var(--space-4);
  }
  .panel {
    max-width: 100%;
    max-height: calc(100vh - 32px);
    display: flex;
    flex-direction: column;
    background: var(--c-bg-elevated);
    border: 1px solid var(--c-border-strong);
    border-radius: var(--radius-lg);
    box-shadow: 0 20px 60px var(--c-shadow);
    outline: none;
  }
  .head {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-4) var(--space-4) var(--space-2);
  }
  .head h2 {
    flex: 1;
    font-size: 15px;
    font-weight: 600;
  }
  .content {
    padding: var(--space-2) var(--space-4);
    overflow: auto;
    user-select: text;
  }
  .foot {
    display: flex;
    justify-content: flex-end;
    gap: var(--space-2);
    padding: var(--space-3) var(--space-4) var(--space-4);
  }
</style>
