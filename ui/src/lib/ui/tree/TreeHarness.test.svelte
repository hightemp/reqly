<script lang="ts">
  import Tree from './Tree.svelte';
  import type { TreeItem } from './model';

  let props: Record<string, unknown> = $props();
  const items: TreeItem[] = [
    { id: 'api', parentId: null, isFolder: true, label: 'api' },
    { id: 'users', parentId: 'api', isFolder: true, label: 'users' },
    { id: 'list', parentId: 'users', isFolder: false, label: 'list' },
    { id: 'health', parentId: 'api', isFolder: false, label: 'health' },
    { id: 'readme', parentId: null, isFolder: false, label: 'readme' },
  ];
  // svelte-ignore state_referenced_locally
  let expanded = $state((props.expanded as string[]) ?? []);
  // svelte-ignore state_referenced_locally
  let renaming = $state((props.renaming as string | null) ?? null);
</script>

<div style="height: 400px">
  <Tree
    {items}
    label="Requests"
    bind:expanded
    bind:renaming
    height={400}
    onopen={props.onopen as never}
    onactivate={props.onactivate as never}
    onmove={props.onmove as never}
    onrename={props.onrename as never}
    oncontextmenu={props.oncontextmenu as never}
    onkey={props.onkey as never}
  >
    {#snippet row(r)}<span>{r.item.label}</span>{/snippet}
  </Tree>
</div>
