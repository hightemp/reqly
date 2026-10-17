// Workspaces and collections; the active pair drives the sidebar.

import type { Collection } from '$lib/api/generated/Collection';
import type { Workspace } from '$lib/api/generated/Workspace';
import type { Transport } from '$lib/api/transport';
import type { PreferencesStore } from '$lib/preferences.svelte';

export class WorkspaceStore {
  workspaces = $state<Workspace[]>([]);
  collections = $state<Collection[]>([]);
  activeWorkspaceId = $state<string | null>(null);
  activeCollectionId = $state<string | null>(null);

  readonly activeWorkspace = $derived(
    this.workspaces.find((w) => w.id === this.activeWorkspaceId) ?? null,
  );
  readonly activeCollection = $derived(
    this.collections.find((c) => c.id === this.activeCollectionId) ?? null,
  );

  private unsubscribe: (() => void) | null = null;

  constructor(
    private readonly transport: Transport,
    private readonly prefs: PreferencesStore,
  ) {}

  async load() {
    this.workspaces = await this.transport.call('workspace.ensureDefault', {});
    const wanted = this.prefs.value.activeWorkspaceId;
    const ws = this.workspaces.find((w) => w.id === wanted) ?? this.workspaces[0];
    if (ws) await this.selectWorkspace(ws.id, this.prefs.value.activeCollectionId);
    this.unsubscribe ??= this.transport.subscribe((e) => {
      if (e.topic === 'workspaces.changed') void this.reloadWorkspaces();
      if (e.topic === 'collections.changed') {
        const payload = e.payload as { workspaceId?: string } | null;
        if (payload?.workspaceId === this.activeWorkspaceId) void this.reloadCollections();
      }
    });
  }

  private async reloadWorkspaces() {
    this.workspaces = await this.transport.call('workspace.list', {});
    if (!this.workspaces.some((w) => w.id === this.activeWorkspaceId) && this.workspaces[0]) {
      await this.selectWorkspace(this.workspaces[0].id);
    }
  }

  private async reloadCollections() {
    if (!this.activeWorkspaceId) return;
    this.collections = await this.transport.call('collection.list', {
      workspaceId: this.activeWorkspaceId,
    });
    if (!this.collections.some((c) => c.id === this.activeCollectionId)) {
      this.activeCollectionId = this.collections[0]?.id ?? null;
    }
  }

  async selectWorkspace(id: string, preferredCollection?: string | null) {
    this.activeWorkspaceId = id;
    this.collections = await this.transport.call('collection.list', { workspaceId: id });
    const col = this.collections.find((c) => c.id === preferredCollection) ?? this.collections[0];
    this.activeCollectionId = col?.id ?? null;
    await this.prefs.update({ activeWorkspaceId: id, activeCollectionId: this.activeCollectionId });
  }

  async selectCollection(id: string) {
    this.activeCollectionId = id;
    await this.prefs.update({ activeCollectionId: id });
  }

  async createWorkspace(name: string) {
    const ws = await this.transport.call('workspace.create', { name });
    await this.transport.call('collection.create', { workspaceId: ws.id, name });
    this.workspaces = await this.transport.call('workspace.list', {});
    await this.selectWorkspace(ws.id);
  }

  async renameWorkspace(id: string, name: string) {
    await this.transport.call('workspace.rename', { id, name });
    this.workspaces = this.workspaces.map((w) => (w.id === id ? { ...w, name } : w));
  }

  async deleteWorkspace(id: string) {
    await this.transport.call('workspace.delete', { id });
    await this.reloadWorkspaces();
  }

  async createCollection(name: string) {
    if (!this.activeWorkspaceId) return;
    const col = await this.transport.call('collection.create', {
      workspaceId: this.activeWorkspaceId,
      name,
    });
    await this.reloadCollections();
    await this.selectCollection(col.id);
  }

  async renameCollection(id: string, name: string) {
    await this.transport.call('collection.rename', { id, name });
    this.collections = this.collections.map((c) => (c.id === id ? { ...c, name } : c));
  }

  async deleteCollection(id: string) {
    await this.transport.call('collection.delete', { id });
    await this.reloadCollections();
  }

  destroy() {
    this.unsubscribe?.();
  }
}
