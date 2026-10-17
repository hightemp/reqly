<!-- Sidebar: workspace/collection pickers, search, creation and the request tree. -->
<script lang="ts">
  import { onMount } from 'svelte';
  import { t } from '$lib/i18n';
  import type { Services } from '$lib/services.svelte';
  import Icon from '$lib/ui/Icon.svelte';
  import IconButton from '$lib/ui/IconButton.svelte';
  import { dialogs } from '$lib/ui/overlay/dialogs.svelte';
  import { menus, type MenuEntry } from '$lib/ui/overlay/menus.svelte';
  import Select from '$lib/ui/overlay/Select.svelte';
  import { bindTreeHotkeys, createFolder, createRequest, type TreeActionHooks } from './actions';
  import CategoriesView from './CategoriesView.svelte';
  import { makeFilter, parseQuery } from './filter';
  import RequestTree from './RequestTree.svelte';

  interface Props {
    services: Services;
    onFolderSettings: (id: string) => void;
    onCollapse: () => void;
    onOpen: (modal: 'cookies' | 'trash' | 'log') => void;
  }

  let { services, onFolderSettings, onCollapse, onOpen }: Props = $props();
  const ws = $derived(services.workspace);
  const prefs = $derived(services.prefs);

  let query = $state('');
  let searchInput: HTMLInputElement | null = $state(null);
  let categories: CategoriesView | undefined = $state();
  const filter = $derived(makeFilter(query));
  const words = $derived(parseQuery(query).words);
  const hooks: TreeActionHooks = {
    openFolderSettings: (id) => onFolderSettings(id),
    reveal: (id) => categories?.reveal(id),
  };

  async function askName(title: string, value = '') {
    const name = await dialogs.prompt({
      title,
      label: t('actions.name'),
      value,
      validate: (v) => (v.trim() ? null : t('actions.nameRequired')),
    });
    return name?.trim() || null;
  }

  function workspaceMenu(anchor: HTMLElement) {
    const current = ws.activeWorkspace;
    const entries: MenuEntry[] = [
      {
        id: 'new',
        label: t('workspace.new'),
        onSelect: async () => {
          const name = await askName(t('workspace.new'));
          if (name) await ws.createWorkspace(name);
        },
      },
    ];
    entries.push(
      { id: 'cookies', label: t('cookieManager.title'), onSelect: () => onOpen('cookies') },
      { id: 'log', label: t('log.title'), onSelect: () => onOpen('log') },
      { separator: true },
    );
    if (current) {
      entries.push(
        {
          id: 'rename',
          label: t('workspace.rename'),
          onSelect: async () => {
            const name = await askName(t('workspace.rename'), current.name);
            if (name) await ws.renameWorkspace(current.id, name);
          },
        },
        { separator: true },
        {
          id: 'delete',
          label: t('workspace.delete'),
          danger: true,
          disabled: ws.workspaces.length < 2,
          onSelect: async () => {
            const ok = await dialogs.confirm({
              title: t('workspace.deleteTitle', { name: current.name }),
              message: t('workspace.deleteMessage'),
              confirmLabel: t('tree.delete'),
              danger: true,
            });
            if (ok) await ws.deleteWorkspace(current.id);
          },
        },
      );
    }
    menus.openBelow(anchor, entries);
  }

  function collectionMenu(anchor: HTMLElement) {
    const current = ws.activeCollection;
    const entries: MenuEntry[] = [
      {
        id: 'new',
        label: t('collection.new'),
        onSelect: async () => {
          const name = await askName(t('collection.new'));
          if (name) await ws.createCollection(name);
        },
      },
    ];
    entries.push(
      { id: 'trash', label: t('trash.title'), onSelect: () => onOpen('trash') },
      { separator: true },
    );
    if (current) {
      entries.push(
        {
          id: 'rename',
          label: t('collection.rename'),
          onSelect: async () => {
            const name = await askName(t('collection.rename'), current.name);
            if (name) await ws.renameCollection(current.id, name);
          },
        },
        { separator: true },
        {
          id: 'delete',
          label: t('collection.delete'),
          danger: true,
          disabled: ws.collections.length < 2,
          onSelect: async () => {
            const ok = await dialogs.confirm({
              title: t('collection.deleteTitle', { name: current.name }),
              message: t('workspace.deleteMessage'),
              confirmLabel: t('tree.delete'),
              danger: true,
            });
            if (ok) await ws.deleteCollection(current.id);
          },
        },
      );
    }
    menus.openBelow(anchor, entries);
  }

  function createMenu(anchor: HTMLElement) {
    const at = services.tree.selected.length === 1 ? services.tree.selected[0]! : null;
    menus.openBelow(anchor, [
      {
        id: 'request',
        label: t('actions.newRequest'),
        onSelect: () => void createRequest(services, at),
      },
      {
        id: 'folder',
        label: t('actions.newFolder'),
        onSelect: () => void createFolder(services, at),
      },
    ]);
  }

  onMount(() => {
    const offs = [
      bindTreeHotkeys(services, hooks),
      services.hotkeys.on('tree.search', () => searchInput?.focus()),
    ];
    return () => offs.forEach((off) => off());
  });
</script>

<aside class="sidebar" aria-label={t('sidebar.title')}>
  <div class="pickers">
    <div class="picker">
      <Select
        compact
        label={t('workspace.label')}
        value={ws.activeWorkspaceId ?? ''}
        options={ws.workspaces.map((w) => ({ value: w.id, label: w.name }))}
        onchange={(id) => void ws.selectWorkspace(id)}
      />
      <IconButton
        icon="more"
        label={t('workspace.menu')}
        onclick={(e) => workspaceMenu(e.currentTarget)}
      />
    </div>
    <div class="picker">
      <Select
        compact
        label={t('collection.label')}
        value={ws.activeCollectionId ?? ''}
        options={ws.collections.map((c) => ({ value: c.id, label: c.name }))}
        onchange={(id) => void ws.selectCollection(id)}
      />
      <IconButton
        icon="more"
        label={t('collection.menu')}
        onclick={(e) => collectionMenu(e.currentTarget)}
      />
    </div>
  </div>

  <div class="toolbar">
    <label class="search">
      <Icon name="search" size={14} />
      <input
        bind:this={searchInput}
        bind:value={query}
        type="text"
        spellcheck="false"
        aria-label={t('sidebar.search')}
        placeholder={t('sidebar.searchPlaceholder')}
        onkeydown={(e) => {
          if (e.key === 'Escape') {
            query = '';
            searchInput?.blur();
          }
        }}
      />
    </label>
    <IconButton
      icon="plus"
      label={t('sidebar.create')}
      onclick={(e) => createMenu(e.currentTarget)}
    />
    <IconButton
      icon={prefs.value.sidebarMode === 'tree' ? 'list' : 'tree'}
      label={prefs.value.sidebarMode === 'tree'
        ? t('sidebar.modeCategories')
        : t('sidebar.modeTree')}
      onclick={() =>
        void prefs.update({
          sidebarMode: prefs.value.sidebarMode === 'tree' ? 'categories' : 'tree',
        })}
    />
    <IconButton icon="sidebar" label={t('sidebar.collapse')} onclick={onCollapse} />
  </div>

  <div class="body">
    {#if prefs.value.sidebarMode === 'tree'}
      <RequestTree {services} {filter} {words} {hooks} />
    {:else}
      <CategoriesView bind:this={categories} {services} {filter} {words} {hooks} />
    {/if}
  </div>
</aside>

<style>
  .sidebar {
    height: 100%;
    display: flex;
    flex-direction: column;
    background: var(--c-bg-elevated);
  }
  .pickers {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: var(--space-2) var(--space-2) var(--space-1);
    border-bottom: 1px solid var(--c-border);
  }
  .picker {
    display: flex;
    align-items: center;
    gap: 2px;
  }
  .picker :global(.select) {
    flex: 1;
    font-weight: 600;
  }
  .toolbar {
    display: flex;
    align-items: center;
    gap: 2px;
    padding: var(--space-2);
  }
  .search {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: var(--space-1);
    height: var(--control-height);
    padding: 0 var(--space-2);
    border-radius: var(--radius-sm);
    border: 1px solid var(--c-border);
    background: var(--c-bg-sunken);
    color: var(--c-fg-subtle);
  }
  .search:focus-within {
    border-color: var(--c-accent);
  }
  .search input {
    flex: 1;
    min-width: 0;
    border: 0;
    background: transparent;
    color: var(--c-fg);
    outline: none;
  }
  .body {
    flex: 1;
    min-height: 0;
  }
</style>
