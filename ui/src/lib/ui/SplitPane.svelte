<!--
  Two panes with a draggable, keyboard-accessible separator.
  `size` is the pixel size of the first pane (width when horizontal).
-->
<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    first: Snippet;
    second: Snippet;
    /** `horizontal` puts panes side by side; `vertical` stacks them. */
    direction?: 'horizontal' | 'vertical';
    size?: number;
    defaultSize?: number;
    min?: number;
    max?: number;
    step?: number;
    collapsed?: boolean;
    /** Accessible name of the separator. */
    label: string;
    onResize?: (size: number) => void;
  }

  let {
    first,
    second,
    direction = 'horizontal',
    size = $bindable(280),
    defaultSize,
    min = 120,
    max = 2000,
    step = 10,
    collapsed = false,
    label,
    onResize,
  }: Props = $props();

  const initial = size;
  let dragStart: { pointer: number; size: number } | null = $state(null);
  const horizontal = $derived(direction === 'horizontal');

  function setSize(next: number) {
    const clamped = Math.round(Math.min(max, Math.max(min, next)));
    if (clamped === size) return;
    size = clamped;
    onResize?.(clamped);
  }

  function onKeyDown(e: KeyboardEvent) {
    const amount = e.shiftKey ? step * 5 : step;
    const [dec, inc] = horizontal ? ['ArrowLeft', 'ArrowRight'] : ['ArrowUp', 'ArrowDown'];
    if (e.key === dec) setSize(size - amount);
    else if (e.key === inc) setSize(size + amount);
    else if (e.key === 'Home') setSize(min);
    else if (e.key === 'End') setSize(max);
    else return;
    e.preventDefault();
  }

  const coord = (e: PointerEvent) => (horizontal ? e.clientX : e.clientY);

  function onPointerDown(e: PointerEvent) {
    if (e.button !== 0) return;
    dragStart = { pointer: coord(e), size };
    (e.currentTarget as HTMLElement).setPointerCapture?.(e.pointerId);
    e.preventDefault();
  }

  function onPointerMove(e: PointerEvent) {
    if (dragStart) setSize(dragStart.size + coord(e) - dragStart.pointer);
  }

  function onPointerUp(e: PointerEvent) {
    if (!dragStart) return;
    (e.currentTarget as HTMLElement).releasePointerCapture?.(e.pointerId);
    dragStart = null;
  }
</script>

<div class="split" class:vertical={!horizontal} class:dragging={dragStart !== null}>
  {#if !collapsed}
    <div class="pane first" style:flex-basis="{size}px">
      {@render first()}
    </div>
    <!-- A focusable separator is an ARIA widget; Svelte treats the role as static. -->
    <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
    <div
      class="separator"
      role="separator"
      tabindex="0"
      aria-label={label}
      aria-orientation={horizontal ? 'vertical' : 'horizontal'}
      aria-valuenow={size}
      aria-valuemin={min}
      aria-valuemax={max}
      onkeydown={onKeyDown}
      onpointerdown={onPointerDown}
      onpointermove={onPointerMove}
      onpointerup={onPointerUp}
      onpointercancel={onPointerUp}
      ondblclick={() => setSize(defaultSize ?? initial)}
    ></div>
  {/if}
  <div class="pane second">
    {@render second()}
  </div>
</div>

<style>
  .split {
    display: flex;
    width: 100%;
    height: 100%;
    min-width: 0;
    min-height: 0;
  }
  .split.vertical {
    flex-direction: column;
  }
  .split.dragging {
    cursor: col-resize;
    user-select: none;
  }
  .split.vertical.dragging {
    cursor: row-resize;
  }
  .pane {
    min-width: 0;
    min-height: 0;
    overflow: hidden;
  }
  .first {
    flex: 0 0 auto;
  }
  .second {
    flex: 1 1 0;
  }
  .separator {
    flex: 0 0 1px;
    position: relative;
    background: var(--c-border);
    cursor: col-resize;
    outline: none;
    touch-action: none;
  }
  .vertical > .separator {
    cursor: row-resize;
  }
  /* Wider invisible hit area around the 1px line. */
  .separator::after {
    content: '';
    position: absolute;
    inset: 0 -3px;
    z-index: 1;
  }
  .vertical > .separator::after {
    inset: -3px 0;
  }
  .separator:hover,
  .dragging > .separator,
  .separator:focus-visible {
    background: var(--c-accent);
  }
</style>
