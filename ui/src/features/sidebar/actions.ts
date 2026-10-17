// Menu entries and hotkeys for tree items, shared by both sidebar modes.

import { t } from '$lib/i18n';
import type { Services } from '$lib/services.svelte';
import type { SortMode } from '$lib/stores/tree.svelte';
import { dialogs } from '$lib/ui/overlay/dialogs.svelte';
import type { MenuEntry } from '$lib/ui/overlay/menus.svelte';
import { formatCombo } from '$lib/hotkeys/hotkeys';

export interface TreeActionHooks {
  /** Opens folder settings (headers, auth, settings). */
  openFolderSettings: (id: string) => void;
  /** In categories mode: select the request's folder in the category tree. */
  reveal?: (id: string) => void;
}

const SORT_MODES: SortMode[] = ['manual', 'name', 'method', 'created', 'updated'];

export async function createRequest(s: Services, at: string | null) {
  const id = await s.tree.createRequest(at, t('names.newRequest'));
  if (id) s.tabs.open(id);
  return id;
}

export async function createFolder(s: Services, at: string | null) {
  return s.tree.createFolder(at, t('names.newFolder'));
}

export async function promptRename(s: Services, id: string) {
  const item = s.tree.byId.get(id);
  if (!item) return;
  const name = await dialogs.prompt({
    title: t('actions.renameTitle'),
    label: t('actions.name'),
    value: item.label,
    validate: (v) => (v.trim() ? null : t('actions.nameRequired')),
  });
  if (name) await s.tree.rename(id, name.trim());
}

export function treeMenu(s: Services, ids: string[], hooks: TreeActionHooks): MenuEntry[] {
  const k = (action: string) => {
    const combo = s.hotkeys.combosFor(action)[0];
    return combo ? formatCombo(combo, s.platform) : undefined;
  };
  const single = ids.length === 1 ? s.tree.byId.get(ids[0]!) : undefined;
  const at = ids.length === 1 ? ids[0]! : null;
  const anyRequest = ids.some((id) => s.tree.byId.get(id) && !s.tree.byId.get(id)!.isFolder);
  const folderId = single?.isFolder ? single.id : (single?.parentId ?? null);

  const entries: MenuEntry[] = [
    { heading: t('actions.create') },
    {
      id: 'new-request',
      label: t('actions.newRequest'),
      shortcut: k('request.new'),
      onSelect: () => void createRequest(s, at),
    },
    {
      id: 'new-folder',
      label: t('actions.newFolder'),
      shortcut: k('folder.new'),
      onSelect: () => void createFolder(s, at),
    },
    { separator: true },
  ];
  if (single) {
    entries.push({
      id: 'rename',
      label: t('actions.rename'),
      shortcut: k('tree.rename'),
      onSelect: () => (s.tree.renaming = single.id),
    });
  }
  entries.push(
    {
      id: 'duplicate',
      label: t('actions.duplicate'),
      shortcut: k('tree.duplicate'),
      onSelect: () => void s.tree.duplicate(ids),
    },
    {
      id: 'cut',
      label: t('actions.cut'),
      shortcut: k('tree.cut'),
      onSelect: () => s.tree.cut(ids),
    },
    {
      id: 'copy',
      label: t('actions.copy'),
      shortcut: k('tree.copy'),
      onSelect: () => s.tree.copy(ids),
    },
    {
      id: 'paste',
      label: t('actions.paste'),
      shortcut: k('tree.paste'),
      disabled: !s.tree.clipboard,
      onSelect: () => void s.tree.paste(at),
    },
  );
  if (anyRequest) {
    entries.push({
      id: 'curl',
      label: t('actions.copyCurl'),
      onSelect: () => void s.tree.copyAsCurl(ids),
    });
  }
  if (single && !single.isFolder && hooks.reveal) {
    entries.push({
      id: 'reveal',
      label: t('actions.reveal'),
      onSelect: () => hooks.reveal!(single.id),
    });
  }
  if (single?.isFolder) {
    entries.push(
      { separator: true },
      {
        id: 'folder-settings',
        label: t('actions.folderSettings'),
        onSelect: () => hooks.openFolderSettings(single.id),
      },
    );
  }
  entries.push(
    { separator: true },
    {
      id: 'sort',
      label: t('actions.sort'),
      children: SORT_MODES.map((mode) => ({
        id: `sort-${mode}`,
        // i18n-dynamic: sort.
        label: t(`sort.${mode}`),
        checked: s.tree.sortModeOf(folderId) === mode,
        onSelect: () => void s.tree.setSortMode(folderId, mode),
      })),
    },
    { separator: true },
    {
      id: 'delete',
      label: t('actions.delete'),
      shortcut: k('tree.delete'),
      danger: true,
      onSelect: () => void s.tree.remove(ids),
    },
  );
  return entries;
}

/** Binds tree-scope hotkeys to the current selection. Returns an unbind function. */
export function bindTreeHotkeys(s: Services, hooks: TreeActionHooks): () => void {
  const sel = () => s.tree.selected;
  const one = () => (s.tree.selected.length === 1 ? s.tree.selected[0]! : null);
  const offs = [
    s.hotkeys.on('tree.rename', () => {
      const id = one();
      if (id) s.tree.renaming = id;
    }),
    s.hotkeys.on('tree.delete', () => void s.tree.remove(sel())),
    s.hotkeys.on('tree.duplicate', () => void s.tree.duplicate(sel())),
    s.hotkeys.on('tree.cut', () => s.tree.cut(sel())),
    s.hotkeys.on('tree.copy', () => s.tree.copy(sel())),
    s.hotkeys.on('tree.paste', () => void s.tree.paste(one())),
  ];
  void hooks;
  return () => offs.forEach((off) => off());
}
