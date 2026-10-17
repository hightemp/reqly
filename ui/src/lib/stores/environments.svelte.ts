// Environments of the active collection and its workspace, plus the
// variable/function catalogs used for template highlighting and completion.

import type { Environment } from '$lib/api/generated/Environment';
import type { EnvironmentKind } from '$lib/api/generated/EnvironmentKind';
import type { EnvironmentUpdateInput } from '$lib/api/generated/EnvironmentUpdateInput';
import type { TemplateFunctionsOutput } from '$lib/api/generated/TemplateFunctionsOutput';
import type { VariableInfo } from '$lib/api/generated/VariableInfo';
import type { Transport } from '$lib/api/transport';

export type EnvironmentPatch = Omit<EnvironmentUpdateInput, 'id'>;

export class EnvironmentStore {
  collectionId = $state<string | null>(null);
  collection = $state<Environment[]>([]);
  global = $state<Environment[]>([]);
  activeId = $state<string | null>(null);
  activeGlobalId = $state<string | null>(null);
  catalog = $state<TemplateFunctionsOutput | null>(null);
  /** Bumped whenever variables may have changed (re-check highlighted templates). */
  revision = $state(0);

  // eslint-disable-next-line svelte/prefer-svelte-reactivity -- cache, read through async methods only
  private variableCache = new Map<string, Promise<VariableInfo[]>>();
  // eslint-disable-next-line svelte/prefer-svelte-reactivity -- plain listener registry
  private listeners = new Set<() => void>();
  private unsubscribe: () => void;

  constructor(private readonly transport: Transport) {
    this.unsubscribe = transport.subscribe((e) => {
      if (e.topic === 'environments.changed') {
        const p = e.payload as { collectionId?: string | null };
        if (!p.collectionId || p.collectionId === this.collectionId) void this.reload();
      } else if (e.topic === 'tree.changed') {
        // Folder and request variables live in the tree.
        this.invalidate();
      }
    });
  }

  get base(): Environment | null {
    return this.collection.find((e) => e.kind === 'base') ?? null;
  }

  get subs(): Environment[] {
    return this.collection.filter((e) => e.kind === 'sub');
  }

  get active(): Environment | null {
    return this.collection.find((e) => e.id === this.activeId) ?? null;
  }

  get activeGlobal(): Environment | null {
    return this.global.find((e) => e.id === this.activeGlobalId) ?? null;
  }

  get workspaceId(): string | null {
    return this.base?.workspaceId ?? null;
  }

  find(id: string): Environment | null {
    return [...this.collection, ...this.global].find((e) => e.id === id) ?? null;
  }

  async load(collectionId: string) {
    this.collectionId = collectionId;
    await this.reload();
  }

  async reload() {
    const id = this.collectionId;
    if (!id) return;
    const out = await this.transport.call('environment.list', { collectionId: id });
    if (id !== this.collectionId) return;
    this.collection = out.collection;
    this.global = out.global;
    this.activeId = out.activeId;
    this.activeGlobalId = out.activeGlobalId;
    this.invalidate();
  }

  private invalidate() {
    this.variableCache.clear();
    this.revision++;
    this.listeners.forEach((l) => l());
  }

  /** Called whenever variables may have changed; returns an unsubscribe function. */
  onChange(listener: () => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  async loadCatalog(): Promise<TemplateFunctionsOutput> {
    if (!this.catalog) this.catalog = await this.transport.call('template.functions', {});
    return this.catalog;
  }

  /** Variables visible from `nodeId` (or the collection when `null`). */
  variables(nodeId: string | null): Promise<VariableInfo[]> {
    const collectionId = this.collectionId;
    if (!collectionId) return Promise.resolve([]);
    const key = nodeId ?? '';
    let hit = this.variableCache.get(key);
    if (!hit) {
      hit = this.transport
        .call('template.variables', { collectionId, nodeId: nodeId ?? undefined })
        .catch(() => []);
      this.variableCache.set(key, hit);
    }
    return hit;
  }

  preview(text: string, nodeId: string | null): Promise<string> {
    const collectionId = this.collectionId;
    if (!collectionId) return Promise.reject(new Error('no collection'));
    return this.transport
      .call('template.preview', { collectionId, nodeId: nodeId ?? undefined, text })
      .then((r) => r.value);
  }

  async setActive(environmentId: string | null) {
    if (!this.collectionId) return;
    this.activeId = environmentId;
    this.invalidate();
    await this.transport.call('environment.setActive', {
      collectionId: this.collectionId,
      environmentId,
    });
  }

  async setActiveGlobal(environmentId: string | null) {
    const workspaceId = this.workspaceId;
    if (!workspaceId) return;
    this.activeGlobalId = environmentId;
    this.invalidate();
    await this.transport.call('environment.setActiveGlobal', { workspaceId, environmentId });
  }

  async create(kind: Exclude<EnvironmentKind, 'base'>, name: string): Promise<Environment | null> {
    const workspaceId = this.workspaceId;
    if (!workspaceId || !this.collectionId) return null;
    const env = await this.transport.call('environment.create', {
      workspaceId,
      collectionId: kind === 'global' ? null : this.collectionId,
      kind,
      name,
    });
    await this.reload();
    return env;
  }

  async update(id: string, patch: EnvironmentPatch) {
    const env = await this.transport.call('environment.update', { id, ...patch });
    const replace = (list: Environment[]) => list.map((e) => (e.id === id ? env : e));
    this.collection = replace(this.collection);
    this.global = replace(this.global);
    this.invalidate();
    return env;
  }

  async duplicate(id: string) {
    const env = await this.transport.call('environment.duplicate', { id });
    await this.reload();
    return env;
  }

  async remove(id: string) {
    await this.transport.call('environment.delete', { id });
    await this.reload();
  }

  destroy() {
    this.unsubscribe();
  }
}
