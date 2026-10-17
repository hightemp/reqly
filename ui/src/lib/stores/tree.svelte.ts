// The tree of the active collection: data, UI state (expanded, selection,
// rename, clipboard, sort modes) and tree operations.

import type { ApiPlacement } from '$lib/api/generated/ApiPlacement';
import type { NodeSummary } from '$lib/api/generated/NodeSummary';
import type { Transport } from '$lib/api/transport';
import { errorMessage } from '$lib/errors';
import { t } from '$lib/i18n';
import { dialogs } from '$lib/ui/overlay/dialogs.svelte';
import { toasts } from '$lib/ui/overlay/toasts.svelte';
import type { DropPosition, TreeItem } from '$lib/ui/tree/model';
import { normalizeSelection } from '$lib/ui/tree/model';

export interface RequestTreeItem extends TreeItem {
  method: string | null;
  url: string | null;
  createdAt: number;
  updatedAt: number;
}

export type SortMode = 'manual' | 'name' | 'method' | 'created' | 'updated';

interface UiState {
  expanded: string[];
  /** Sort mode per folder id (`root` for the collection root). */
  sort: Record<string, SortMode>;
}

const ROOT = 'root';

function compare(mode: SortMode, a: RequestTreeItem, b: RequestTreeItem): number {
  // Folders first in every automatic mode.
  if (mode !== 'manual' && a.isFolder !== b.isFolder) return a.isFolder ? -1 : 1;
  switch (mode) {
    case 'name':
      return a.label.localeCompare(b.label, undefined, { numeric: true, sensitivity: 'base' });
    case 'method':
      return (a.method ?? '').localeCompare(b.method ?? '') || a.label.localeCompare(b.label);
    case 'created':
      return b.createdAt - a.createdAt;
    case 'updated':
      return b.updatedAt - a.updatedAt;
    default:
      return 0;
  }
}

export class TreeStore {
  collectionId = $state<string | null>(null);
  nodes = $state<NodeSummary[]>([]);
  expanded = $state<string[]>([]);
  selected = $state<string[]>([]);
  active = $state<string | null>(null);
  renaming = $state<string | null>(null);
  clipboard = $state<{ mode: 'cut' | 'copy'; ids: string[] } | null>(null);
  sort = $state<Record<string, SortMode>>({});
  loading = $state(false);

  /** Nodes as tree items, children ordered by each folder's sort mode. */
  readonly items = $derived.by<RequestTreeItem[]>(() => {
    const base: RequestTreeItem[] = this.nodes.map((n) => ({
      id: n.id,
      parentId: n.parentId,
      isFolder: n.kind === 'folder',
      label: n.name,
      method: n.method,
      url: n.url,
      createdAt: n.createdAt,
      updatedAt: n.updatedAt,
    }));
    // eslint-disable-next-line svelte/prefer-svelte-reactivity -- local temporary, not state
    const groups = new Map<string, RequestTreeItem[]>();
    for (const item of base) {
      const key = item.parentId ?? ROOT;
      groups.set(key, [...(groups.get(key) ?? []), item]);
    }
    const out: RequestTreeItem[] = [];
    for (const [key, list] of groups) {
      const mode = this.sort[key] ?? 'manual';
      out.push(...(mode === 'manual' ? list : [...list].sort((a, b) => compare(mode, a, b))));
    }
    return out;
  });

  // eslint-disable-next-line svelte/prefer-svelte-reactivity -- local temporary, not state
  readonly byId = $derived(new Map(this.items.map((i) => [i.id, i])));

  private unsubscribe: (() => void) | null = null;
  private saveTimer: ReturnType<typeof setTimeout> | undefined;

  constructor(private readonly transport: Transport) {}

  async load(collectionId: string) {
    if (this.collectionId !== collectionId) {
      this.selected = [];
      this.active = null;
      this.renaming = null;
    }
    this.collectionId = collectionId;
    this.loading = true;
    try {
      const [nodes, ui] = await Promise.all([
        this.transport.call('tree.list', { collectionId }),
        this.transport.call('settings.get', { key: this.uiKey() }),
      ]);
      if (this.collectionId !== collectionId) return;
      this.nodes = nodes;
      const state = (ui.value ?? {}) as Partial<UiState>;
      this.expanded = Array.isArray(state.expanded) ? state.expanded : [];
      this.sort = state.sort && typeof state.sort === 'object' ? state.sort : {};
    } finally {
      this.loading = false;
    }
    this.unsubscribe ??= this.transport.subscribe((e) => {
      const payload = e.payload as { collectionIds?: string[] } | null;
      if (
        e.topic === 'tree.changed' &&
        this.collectionId &&
        payload?.collectionIds?.includes(this.collectionId)
      ) {
        void this.reload();
      }
    });
  }

  async reload() {
    if (!this.collectionId) return;
    this.nodes = await this.transport.call('tree.list', { collectionId: this.collectionId });
    // eslint-disable-next-line svelte/prefer-svelte-reactivity -- local temporary, not state
    const ids = new Set(this.nodes.map((n) => n.id));
    this.selected = this.selected.filter((id) => ids.has(id));
  }

  private uiKey() {
    return `ui.tree.${this.collectionId}`;
  }

  private scheduleSave() {
    clearTimeout(this.saveTimer);
    this.saveTimer = setTimeout(() => void this.flushUiState(), 300);
  }

  async flushUiState() {
    clearTimeout(this.saveTimer);
    if (!this.collectionId) return;
    const value: UiState = { expanded: this.expanded, sort: this.sort };
    await this.transport.call('settings.set', { key: this.uiKey(), value });
  }

  setExpanded(expanded: string[]) {
    this.expanded = expanded;
    this.scheduleSave();
  }

  expand(id: string) {
    if (!this.expanded.includes(id)) this.setExpanded([...this.expanded, id]);
  }

  sortModeOf(folderId: string | null): SortMode {
    return this.sort[folderId ?? ROOT] ?? 'manual';
  }

  async setSortMode(folderId: string | null, mode: SortMode) {
    this.sort = { ...this.sort, [folderId ?? ROOT]: mode };
    await this.flushUiState();
  }

  parentOf(id: string): string | null {
    return this.byId.get(id)?.parentId ?? null;
  }

  /** Folder names from the root down to the node's parent. */
  pathOf(id: string): string[] {
    const path: string[] = [];
    let parent = this.parentOf(id);
    while (parent) {
      const item = this.byId.get(parent);
      if (!item) break;
      path.unshift(item.label);
      parent = item.parentId;
    }
    return path;
  }

  /** Where new items go when created "at" a node: inside folders, next to requests. */
  private containerFor(id: string | null): { parentId: string | null; placement: ApiPlacement } {
    if (!id) return { parentId: null, placement: { type: 'end' } };
    const item = this.byId.get(id);
    if (item?.isFolder) return { parentId: id, placement: { type: 'end' } };
    return { parentId: item?.parentId ?? null, placement: { type: 'after', id } };
  }

  private async run<T>(op: () => Promise<T>): Promise<T | undefined> {
    try {
      return await op();
    } catch (e) {
      toasts.error(errorMessage(e));
      return undefined;
    }
  }

  async createRequest(at: string | null, name: string): Promise<string | undefined> {
    if (!this.collectionId) return;
    const { parentId, placement } = this.containerFor(at);
    const node = await this.run(() =>
      this.transport.call('node.create', {
        collectionId: this.collectionId!,
        parentId,
        name,
        data: { kind: 'request' } as never,
        placement,
      }),
    );
    if (!node) return;
    if (parentId) this.expand(parentId);
    this.selected = [node.id];
    this.active = node.id;
    await this.reload();
    return node.id;
  }

  async createFolder(at: string | null, name: string): Promise<string | undefined> {
    if (!this.collectionId) return;
    const { parentId, placement } = this.containerFor(at);
    const node = await this.run(() =>
      this.transport.call('node.create', {
        collectionId: this.collectionId!,
        parentId,
        name,
        data: { kind: 'folder' } as never,
        placement,
      }),
    );
    if (!node) return;
    if (parentId) this.expand(parentId);
    this.selected = [node.id];
    this.active = node.id;
    this.renaming = node.id;
    await this.reload();
    return node.id;
  }

  async rename(id: string, name: string) {
    await this.run(() => this.transport.call('node.update', { id, name }));
    this.nodes = this.nodes.map((n) => (n.id === id ? { ...n, name } : n));
  }

  /** Applies a drop: `inside` a folder (or the root when `id` is null), or before/after a node. */
  async move(ids: string[], target: { id: string | null; position: DropPosition }) {
    if (!this.collectionId) return;
    let parentId: string | null;
    let placement: ApiPlacement;
    if (target.id === null) {
      parentId = null;
      placement = { type: 'end' };
    } else if (target.position === 'inside') {
      parentId = target.id;
      placement = { type: 'end' };
      this.expand(target.id);
    } else {
      parentId = this.parentOf(target.id);
      placement = { type: target.position, id: target.id };
      // A manual drop only makes sense in manual order.
      if (this.sortModeOf(parentId) !== 'manual') await this.setSortMode(parentId, 'manual');
    }
    await this.run(() =>
      this.transport.call('tree.move', {
        ids,
        collectionId: this.collectionId!,
        parentId,
        placement,
      }),
    );
    await this.reload();
  }

  async duplicate(ids: string[]) {
    const copies = await this.run(() =>
      this.transport.call('tree.duplicate', { ids: this.normalized(ids) }),
    );
    if (copies?.length) this.selected = copies.map((c) => c.id);
    await this.reload();
  }

  private normalized(ids: string[]) {
    return normalizeSelection(this.items, ids);
  }

  async remove(ids: string[]) {
    const top = this.normalized(ids);
    if (top.length === 0) return;
    const first = this.byId.get(top[0]!);
    const ok = await dialogs.confirm({
      title:
        top.length === 1
          ? t('tree.deleteOneTitle', { name: first?.label ?? '' })
          : t('tree.deleteManyTitle', { count: top.length }),
      message: t('tree.deleteMessage'),
      confirmLabel: t('tree.delete'),
      danger: true,
    });
    if (!ok) return;
    const done = await this.run(() => this.transport.call('tree.delete', { ids: top }));
    if (done === undefined) return;
    this.selected = [];
    await this.reload();
    toasts.show({
      message: t('tree.deleted', { count: top.length }),
      action: {
        label: t('tree.undo'),
        run: () =>
          void this.run(() => this.transport.call('tree.restore', { ids: top })).then(() =>
            this.reload(),
          ),
      },
      timeoutMs: 8000,
    });
  }

  cut(ids: string[]) {
    this.clipboard = { mode: 'cut', ids: this.normalized(ids) };
  }

  copy(ids: string[]) {
    this.clipboard = { mode: 'copy', ids: this.normalized(ids) };
  }

  async paste(at: string | null) {
    if (!this.clipboard || !this.collectionId) return;
    const { parentId, placement } = this.containerFor(at);
    const { mode, ids } = this.clipboard;
    if (mode === 'cut') {
      await this.run(() =>
        this.transport.call('tree.move', {
          ids,
          collectionId: this.collectionId!,
          parentId,
          placement,
        }),
      );
      this.clipboard = null;
    } else {
      const copies = await this.run(() =>
        this.transport.call('tree.copy', {
          ids,
          collectionId: this.collectionId!,
          parentId,
          placement,
        }),
      );
      if (copies?.length) this.selected = copies.map((c) => c.id);
    }
    if (parentId) this.expand(parentId);
    await this.reload();
  }

  /** Copies "curl …" for every selected request (folders are skipped). */
  async copyAsCurl(ids: string[]) {
    const requests = ids.filter((id) => this.byId.get(id) && !this.byId.get(id)!.isFolder);
    const commands: string[] = [];
    for (const id of requests) {
      const out = await this.run(() => this.transport.call('request.toCurl', { requestId: id }));
      if (out) commands.push(out.command);
    }
    if (commands.length === 0) return;
    await navigator.clipboard.writeText(commands.join('\n\n'));
    toasts.show({
      message: t('tree.curlCopied', { count: commands.length }),
      kind: 'success',
      timeoutMs: 2500,
    });
  }

  destroy() {
    clearTimeout(this.saveTimer);
    this.unsubscribe?.();
  }
}
