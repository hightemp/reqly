<!-- Fixed-row-height virtual list. Renders only rows in (and near) the viewport. -->
<script lang="ts" generics="T">
  import type { Snippet } from 'svelte';
  import type { HTMLAttributes } from 'svelte/elements';

  interface Props extends Omit<HTMLAttributes<HTMLDivElement>, 'children'> {
    items: T[];
    rowHeight: number;
    overscan?: number;
    /** Viewport height override (tests); measured otherwise. */
    height?: number;
    key?: (item: T, index: number) => unknown;
    row: Snippet<[T, number]>;
    viewport?: HTMLDivElement | null;
    after?: Snippet;
  }

  let {
    items,
    rowHeight,
    overscan = 8,
    height,
    key = (_, i) => i,
    row,
    viewport = $bindable(null),
    after,
    class: className,
    ...rest
  }: Props = $props();

  let scrollTop = $state(0);
  let measured = $state(0);
  const viewHeight = $derived(height ?? measured);
  const start = $derived(Math.max(0, Math.floor(scrollTop / rowHeight) - overscan));
  const end = $derived(
    Math.min(items.length, Math.ceil((scrollTop + (viewHeight || 600)) / rowHeight) + overscan),
  );
  const visible = $derived(items.slice(start, end));

  export function scrollToIndex(index: number) {
    if (!viewport) return;
    const top = index * rowHeight;
    const bottom = top + rowHeight;
    if (top < viewport.scrollTop) viewport.scrollTop = top;
    else if (bottom > viewport.scrollTop + viewport.clientHeight)
      viewport.scrollTop = bottom - viewport.clientHeight;
  }
</script>

<div
  bind:this={viewport}
  bind:clientHeight={measured}
  class="viewport {className ?? ''}"
  onscroll={(e) => (scrollTop = (e.currentTarget as HTMLDivElement).scrollTop)}
  {...rest}
>
  <div class="spacer" style:height="{items.length * rowHeight}px">
    <div class="window" style:transform="translateY({start * rowHeight}px)">
      {#each visible as item, i (key(item, start + i))}
        {@render row(item, start + i)}
      {/each}
    </div>
  </div>
  {@render after?.()}
</div>

<style>
  .viewport {
    position: relative;
    overflow: auto;
    height: 100%;
    contain: strict;
  }
  .spacer {
    position: relative;
  }
  .window {
    position: absolute;
    inset: 0 0 auto 0;
  }
</style>
