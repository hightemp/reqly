<!-- Tree/list row content: method badge or folder icon, name with search highlight. -->
<script lang="ts">
  import type { RequestTreeItem } from '$lib/stores/tree.svelte';
  import Icon from '$lib/ui/Icon.svelte';
  import MethodBadge from '$lib/ui/MethodBadge.svelte';
  import { highlight } from './filter';

  interface Props {
    item: RequestTreeItem;
    expanded?: boolean;
    words?: string[];
    dimmed?: boolean;
    /** Secondary text, e.g. the folder path in the request list. */
    hint?: string;
  }

  let { item, expanded = false, words = [], dimmed = false, hint }: Props = $props();
  const segments = $derived(highlight(item.label, words));
</script>

<span class="label" class:dimmed>
  {#if item.isFolder}
    <span class="folder"><Icon name={expanded ? 'folder-open' : 'folder'} size={14} /></span>
  {:else}
    <MethodBadge method={item.method ?? 'GET'} />
  {/if}
  <span class="name">
    {#each segments as seg, i (i)}{#if seg.match}<mark>{seg.text}</mark
        >{:else}{seg.text}{/if}{/each}
  </span>
  {#if hint}<span class="hint">{hint}</span>{/if}
</span>

<style>
  .label {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    flex: 1;
  }
  .dimmed {
    opacity: 0.5;
  }
  .folder {
    color: var(--c-fg-subtle);
    min-width: 34px;
    display: flex;
    justify-content: flex-end;
  }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  mark {
    background: var(--c-warning-subtle);
    color: inherit;
    border-radius: 2px;
  }
  .hint {
    margin-left: auto;
    padding-left: var(--space-2);
    font-size: var(--font-size-xs);
    color: var(--c-fg-subtle);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 45%;
  }
</style>
