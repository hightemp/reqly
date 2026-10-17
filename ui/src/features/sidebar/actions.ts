// Menu entries and hotkeys for tree items, shared by both sidebar modes.

import type { Target } from '$lib/api/generated/Target';
import { errorMessage } from '$lib/errors';
import { t } from '$lib/i18n';
import type { Services } from '$lib/services.svelte';
import type { SortMode } from '$lib/stores/tree.svelte';
import { dialogs } from '$lib/ui/overlay/dialogs.svelte';
import type { MenuEntry } from '$lib/ui/overlay/menus.svelte';
import { toasts } from '$lib/ui/overlay/toasts.svelte';
import { formatCombo } from '$lib/hotkeys/hotkeys';
import { interopDialogs } from '../interop/dialogs.svelte';

export interface TreeActionHooks {
  /** Opens folder settings (headers, auth, settings). */
  openFolderSettings: (id: string) => void;
  /** In categories mode: select the request's folder in the category tree. */
  reveal?: (id: string) => void;
}

const SORT_MODES: SortMode[] = ['manual', 'name', 'method', 'created', 'updated'];

export type RequestKind = 'http' | 'graphql' | 'webSocket' | 'sse' | 'grpc';

export const REQUEST_KINDS: RequestKind[] = ['http', 'graphql', 'webSocket', 'sse', 'grpc'];

/** Initial data of a new request of each kind. */
export function newRequestData(kind: RequestKind): Record<string, unknown> {
  switch (kind) {
    case 'graphql':
      return {
        kind: 'request',
        method: 'POST',
        body: {
          kind: 'graphql',
          graphql: { query: 'query {\n  \n}\n', variables: '', operationName: '' },
        },
      };
    case 'sse':
      return { kind: 'request', eventStream: true };
    case 'webSocket':
      return { kind: 'webSocket' };
    case 'grpc':
      return { kind: 'grpc' };
    default:
      return { kind: 'request' };
  }
}

const KIND_NAMES = {
  http: 'names.newRequest',
  graphql: 'names.newGraphql',
  webSocket: 'names.newWebSocket',
  sse: 'names.newSse',
  grpc: 'names.newGrpc',
} as const;

const KIND_LABELS = {
  http: 'actions.newRequest',
  graphql: 'actions.newGraphql',
  webSocket: 'actions.newWebSocket',
  sse: 'actions.newSse',
  grpc: 'actions.newGrpc',
} as const;

export function requestKindLabel(kind: RequestKind): string {
  return t(KIND_LABELS[kind]);
}

export async function createRequest(s: Services, at: string | null, kind: RequestKind = 'http') {
  const id = await s.tree.createRequest(at, t(KIND_NAMES[kind]), newRequestData(kind));
  if (id) s.tabs.open(id);
  return id;
}

/** Menu entries creating each kind of request. */
export function createEntries(s: Services, at: string | null, shortcut?: string): MenuEntry[] {
  return REQUEST_KINDS.map((kind) => ({
    id: `new-${kind}`,
    label: requestKindLabel(kind),
    shortcut: kind === 'http' ? shortcut : undefined,
    onSelect: () => void createRequest(s, at, kind),
  }));
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

/** Quick "Copy as" targets (the generate dialog has all of them). */
const COPY_AS: [Target, string][] = [
  ['curlBash', 'cURL'],
  ['jsFetch', 'JavaScript fetch'],
  ['pythonRequests', 'Python Requests'],
  ['httpie', 'HTTPie'],
];

async function copyAs(s: Services, requestId: string, target: Target) {
  try {
    await s.editor.flush(requestId);
    const { code } = await s.transport.call('codegen.generate', { requestId, target });
    await navigator.clipboard.writeText(code);
    toasts.show({ message: t('codegen.copied'), kind: 'success' });
  } catch (e) {
    toasts.error(errorMessage(e));
  }
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
    ...createEntries(s, at, k('request.new')),
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
  if (single && !single.isFolder) {
    entries.push({
      id: 'copy-as',
      label: t('actions.copyAs'),
      children: [
        ...COPY_AS.map(([target, label]) => ({
          id: `copy-as-${target}`,
          label,
          onSelect: () => void copyAs(s, single.id, target),
        })),
        { separator: true },
        {
          id: 'codegen',
          label: t('actions.generateCode'),
          onSelect: () => interopDialogs.openCodegen(single.id),
        },
      ],
    });
  } else if (anyRequest) {
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
  entries.push({
    id: 'export',
    label: t('actions.export'),
    onSelect: () =>
      interopDialogs.openExport({
        scope: { kind: 'nodes', nodeIds: ids },
        title: single?.label ?? t('actions.exportSelection', { count: ids.length }),
      }),
  });
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
