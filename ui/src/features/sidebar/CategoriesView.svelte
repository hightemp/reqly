<!--
  Categories mode: folder tree on top, below it every request inside the
  selected folder (recursively), grouped by subfolder path or flat.
-->
<script lang="ts">
  import { t } from '$lib/i18n';
  import type { Services } from '$lib/services.svelte';
  import type { RequestTreeItem } from '$lib/stores/tree.svelte';
  import EmptyState from '$lib/ui/EmptyState.svelte';
  import { contextMenu } from '$lib/ui/overlay/menus.svelte';
  import SplitPane from '$lib/ui/SplitPane.svelte';
  import Tree from '$lib/ui/tree/Tree.svelte';
  import { treeMenu, type TreeActionHooks } from './actions';
  import ItemLabel from './ItemLabel.svelte';
  import { buildRequestList, ALL_ID } from './categories';

  interface Props {
    services: Services;
    filter?: (item: RequestTreeItem) => boolean;
    words: string[];
    hooks: TreeActionHooks;
  }

  let { services, filter, words, hooks }: Props = $props();
  const tree = $derived(services.tree);
  const prefs = $derived(services.prefs);

  let category = $state<string>(ALL_ID);
  let categorySelected = $state<string[]>([ALL_ID]);
  let listSelected = $state<string[]>([]);
  let listActive = $state<string | null>(null);

  const allItem: RequestTreeItem = $derived({
    id: ALL_ID,
    parentId: null,
    isFolder: true,
    label: t('sidebar.allRequests'),
    method: null,
    url: null,
    createdAt: 0,
    updatedAt: 0,
  });

  // Folders only, under a virtual "All requests" root.
  const folderItems = $derived<RequestTreeItem[]>([
    allItem,
    ...tree.items.filter((i) => i.isFolder).map((i) => ({ ...i, parentId: i.parentId ?? ALL_ID })),
  ]);
  const expandedWithAll = $derived([ALL_ID, ...tree.expanded]);

  const list = $derived(
    buildRequestList(
      tree.items,
      category === ALL_ID ? null : category,
      prefs.value.flatList,
      filter,
    ),
  );
  const cutIds = $derived(new Set(tree.clipboard?.mode === 'cut' ? tree.clipboard.ids : []));

  $effect(() => {
    if (category !== ALL_ID && !tree.byId.has(category)) category = ALL_ID;
  });

  function realIds(ids: string[]) {
    return ids.filter((id) => id !== ALL_ID && !id.startsWith('group:'));
  }

  export function reveal(id: string) {
    const parent = tree.parentOf(id);
    category = parent ?? ALL_ID;
    categorySelected = [category];
    let p = parent;
    // eslint-disable-next-line svelte/prefer-svelte-reactivity -- local accumulator
    const open = new Set(tree.expanded);
    while (p) {
      open.add(p);
      p = tree.parentOf(p);
    }
    tree.setExpanded([...open]);
    listSelected = [id];
    listActive = id;
  }
</script>

<SplitPane
  direction="vertical"
  bind:size={() => prefs.value.categoriesHeight, (v) => void prefs.update({ categoriesHeight: v })}
  min={80}
  max={900}
  defaultSize={240}
  label={t('sidebar.categories')}
>
  {#snippet first()}
    <Tree
      items={folderItems}
      label={t('sidebar.categories')}
      bind:expanded={
        () => expandedWithAll, (v) => tree.setExpanded(v.filter((id) => id !== ALL_ID))
      }
      bind:selected={categorySelected}
      bind:renaming={tree.renaming}
      draggable={false}
      droppable
      acceptDrop={(_, target) => target.position === 'inside' && target.id !== null}
      onactivate={(id) => (category = id)}
      onrename={(id, name) => void tree.rename(id, name)}
      onmove={(ids, target) =>
        void tree.move(realIds(ids), {
          id: target.id === ALL_ID ? null : target.id,
          position: 'inside',
        })}
      oncontextmenu={(e, ids) => {
        const real = realIds(ids);
        contextMenu(e, treeMenu(services, real.length ? real : [], hooks));
      }}
      onfocuschange={(f) => services.scopes.set('tree', f)}
    >
      {#snippet row(r)}
        <ItemLabel item={r.item} expanded={r.expanded} />
      {/snippet}
    </Tree>
  {/snippet}
  {#snippet second()}
    <Tree
      items={list}
      label={t('sidebar.requests')}
      expanded={list.filter((i) => i.isFolder).map((i) => i.id)}
      bind:selected={listSelected}
      bind:active={listActive}
      bind:renaming={tree.renaming}
      droppable={false}
      onactivate={(id) => {
        if (!id.startsWith('group:')) services.tabs.open(id, { preview: true });
      }}
      onopen={(id) => {
        if (!id.startsWith('group:')) services.tabs.open(id);
      }}
      onrename={(id, name) => void tree.rename(id, name)}
      oncontextmenu={(e, ids) => {
        const real = realIds(ids);
        if (real.length) contextMenu(e, treeMenu(services, real, { ...hooks, reveal }));
      }}
      onfocuschange={(f) => {
        services.scopes.set('tree', f);
        if (f) tree.selected = realIds(listSelected);
      }}
    >
      {#snippet row(r)}
        {#if r.item.isFolder}
          <span class="group">{r.item.label}</span>
        {:else}
          <ItemLabel
            item={r.item}
            {words}
            dimmed={cutIds.has(r.id)}
            hint={prefs.value.flatList ? tree.pathOf(r.id).join(' / ') : undefined}
          />
        {/if}
      {/snippet}
      {#snippet empty()}
        <EmptyState title={filter ? t('sidebar.noMatches') : t('sidebar.emptyCategory')} />
      {/snippet}
    </Tree>
  {/snippet}
</SplitPane>

<style>
  .group {
    font-size: var(--font-size-xs);
    font-weight: 600;
    color: var(--c-fg-subtle);
    text-transform: none;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
