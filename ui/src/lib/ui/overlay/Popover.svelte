<!--
  Floating panel anchored to an element or a point. Rendered in a portal,
  closes on Escape and outside pointer-down, repositions on resize/scroll.
-->
<script lang="ts">
  import { onMount, type Snippet } from 'svelte';
  import type { HTMLAttributes } from 'svelte/elements';
  import { computePosition, type Placement, type Rect } from './position';
  import { portal } from './portal';

  interface Props extends Omit<HTMLAttributes<HTMLDivElement>, 'onclose'> {
    anchor: HTMLElement | Rect;
    placement?: Placement;
    offset?: number;
    /** Match the anchor width (selects, comboboxes). */
    matchWidth?: boolean;
    onclose: (reason: 'escape' | 'outside') => void;
    /** Extra elements that count as "inside" (e.g. the trigger). */
    ignore?: (HTMLElement | null | undefined)[];
    /** Elements matching this selector count as inside (e.g. nested submenus). */
    ignoreSelector?: string;
    element?: HTMLDivElement | null;
    children: Snippet;
  }

  let {
    anchor,
    placement = 'bottom-start',
    offset = 4,
    matchWidth = false,
    onclose,
    ignore = [],
    ignoreSelector,
    element = $bindable(null),
    children,
    class: className,
    ...rest
  }: Props = $props();

  let x = $state(-9999);
  let y = $state(-9999);

  function anchorRect(): Rect {
    if (anchor instanceof HTMLElement) {
      const r = anchor.getBoundingClientRect();
      return { x: r.left, y: r.top, width: r.width, height: r.height };
    }
    return anchor;
  }

  function update() {
    if (!element) return;
    const size = { width: element.offsetWidth, height: element.offsetHeight };
    const pos = computePosition(
      anchorRect(),
      size,
      { width: window.innerWidth, height: window.innerHeight },
      placement,
      offset,
    );
    x = pos.x;
    y = pos.y;
  }

  onMount(() => {
    update();
    const onPointerDown = (e: PointerEvent) => {
      const target = e.target as Node;
      const inside =
        element?.contains(target) ||
        (anchor instanceof HTMLElement && anchor.contains(target)) ||
        ignore.some((el) => el?.contains(target)) ||
        (ignoreSelector !== undefined &&
          target instanceof Element &&
          target.closest(ignoreSelector) !== null);
      if (!inside) onclose('outside');
    };
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.stopPropagation();
        onclose('escape');
      }
    };
    const ro = new ResizeObserver(update);
    if (element) ro.observe(element);
    window.addEventListener('resize', update);
    window.addEventListener('scroll', update, true);
    document.addEventListener('pointerdown', onPointerDown, true);
    element?.addEventListener('keydown', onKeyDown);
    return () => {
      ro.disconnect();
      window.removeEventListener('resize', update);
      window.removeEventListener('scroll', update, true);
      document.removeEventListener('pointerdown', onPointerDown, true);
      element?.removeEventListener('keydown', onKeyDown);
    };
  });
</script>

<div
  use:portal
  bind:this={element}
  class="popover {className ?? ''}"
  style:left="{x}px"
  style:top="{y}px"
  style:min-width={matchWidth && anchor instanceof HTMLElement
    ? `${anchor.offsetWidth}px`
    : undefined}
  {...rest}
>
  {@render children()}
</div>

<style>
  .popover {
    position: fixed;
    z-index: 1000;
    background: var(--c-bg-elevated);
    color: var(--c-fg);
    border: 1px solid var(--c-border-strong);
    border-radius: var(--radius);
    box-shadow: 0 8px 28px var(--c-shadow);
    max-height: calc(100vh - 16px);
    overflow: auto;
  }
</style>
