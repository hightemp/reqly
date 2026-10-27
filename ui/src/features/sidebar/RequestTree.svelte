<!-- Tree mode: folders and requests in one tree. -->
<script lang="ts">
  import { TOUCH_ROW_HEIGHT } from '$lib/ui/touch';
  import { t } from '$lib/i18n';
  import type { Services } from '$lib/services.svelte';
  import type { RequestTreeItem } from '$lib/stores/tree.svelte';
  import EmptyState from '$lib/ui/EmptyState.svelte';
  import Button from '$lib/ui/Button.svelte';
  import { contextMenu } from '$lib/ui/overlay/menus.svelte';
  import Tree from '$lib/ui/tree/Tree.svelte';
  import { createRequest, treeMenu, type TreeActionHooks } from './actions';
  import Avatars from '$lib/ui/Avatars.svelte';
  import ItemLabel from './ItemLabel.svelte';

  interface Props {
    services: Services;
    filter?: (item: RequestTreeItem) => boolean;
    words: string[];
    hooks: TreeActionHooks;
  }

  let { services, filter, words, hooks }: Props = $props();
  const tree = $derived(services.tree);
  const cutIds = $derived(new Set(tree.clipboard?.mode === 'cut' ? tree.clipboard.ids : []));

  function open(id: string, preview: boolean) {
    const item = tree.byId.get(id);
    if (item && !item.isFolder) services.tabs.open(id, { preview });
  }
</script>

<Tree
  items={tree.items}
  label={t('sidebar.title')}
  bind:expanded={() => tree.expanded, (v) => tree.setExpanded(v)}
  bind:selected={tree.selected}
  bind:active={tree.active}
  bind:renaming={tree.renaming}
  {filter}
  rowHeight={services.viewport.touch ? TOUCH_ROW_HEIGHT : 26}
  onactivate={(id) => open(id, true)}
  onopen={(id) => open(id, false)}
  onrename={(id, name) => void tree.rename(id, name)}
  onmove={(ids, target) => void tree.move(ids, target)}
  oncontextmenu={(e, ids) => contextMenu(e, treeMenu(services, ids, hooks))}
  onfocuschange={(f) => services.scopes.set('tree', f)}
>
  {#snippet row(r)}
    <ItemLabel item={r.item} expanded={r.expanded} {words} dimmed={cutIds.has(r.id)} />
    <Avatars people={services.presence.on(r.id)} size={14} max={3} label={t('presence.open')} />
  {/snippet}
  {#snippet empty()}
    <EmptyState title={filter ? t('sidebar.noMatches') : t('sidebar.empty')}>
      {#if !filter}
        <Button size="sm" onclick={() => void createRequest(services, null)}
          >{t('actions.newRequest')}</Button
        >
      {/if}
    </EmptyState>
  {/snippet}
</Tree>
