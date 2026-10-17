// Request/folder drafts with autosave. Saves for one node are serialized so
// the server always ends with the latest local state.

import type { FolderData } from '$lib/api/generated/FolderData';
import type { HttpRequest } from '$lib/api/generated/HttpRequest';
import type { Node } from '$lib/api/generated/Node';
import type { Transport } from '$lib/api/transport';
import { SvelteMap } from 'svelte/reactivity';
import { errorMessage } from '$lib/errors';
import { toasts } from '$lib/ui/overlay/toasts.svelte';

const AUTOSAVE_DELAY_MS = 500;

type RequestData = HttpRequest & { kind: 'request' };
type FolderNodeData = FolderData & { kind: 'folder' };

export class Draft {
  node = $state<Node | null>(null);
  dirty = $state(false);
  saving = $state(false);
  /** Local edit counter; a save only clears `dirty` if nothing changed meanwhile. */
  revision = 0;
  timer: ReturnType<typeof setTimeout> | undefined;
  chain: Promise<void> = Promise.resolve();

  constructor(readonly id: string) {}

  get request(): RequestData | null {
    return this.node?.data.kind === 'request' ? (this.node.data as RequestData) : null;
  }

  get folder(): FolderNodeData | null {
    return this.node?.data.kind === 'folder' ? (this.node.data as FolderNodeData) : null;
  }
}

export class EditorStore {
  private drafts = new SvelteMap<string, Draft>();
  // eslint-disable-next-line svelte/prefer-svelte-reactivity -- in-flight loads, not UI state
  private loading = new Map<string, Promise<Draft>>();
  private unsubscribe: () => void;

  constructor(
    private readonly transport: Transport,
    private readonly autosave: () => boolean,
  ) {
    this.unsubscribe = transport.subscribe((e) => {
      if (e.topic !== 'tree.changed') return;
      const ids = (e.payload as { nodeIds?: string[] } | null)?.nodeIds ?? [];
      for (const id of ids) {
        const draft = this.drafts.get(id);
        if (draft && !draft.dirty && !draft.saving) void this.refresh(draft);
      }
    });
  }

  get(id: string): Draft | undefined {
    return this.drafts.get(id);
  }

  async open(id: string): Promise<Draft> {
    const existing = this.drafts.get(id);
    if (existing) return existing;
    const pending = this.loading.get(id);
    if (pending) return pending;
    const load = (async () => {
      const draft = new Draft(id);
      draft.node = await this.transport.call('node.get', { id });
      this.drafts.set(id, draft);
      return draft;
    })();
    this.loading.set(id, load);
    try {
      return await load;
    } finally {
      this.loading.delete(id);
    }
  }

  private async refresh(draft: Draft) {
    try {
      const fresh = await this.transport.call('node.get', { id: draft.id });
      if (!draft.dirty && (!draft.node || fresh.version > draft.node.version)) draft.node = fresh;
    } catch {
      // Deleted nodes are cleaned up by the tabs store.
    }
  }

  private changed(draft: Draft) {
    draft.dirty = true;
    draft.revision += 1;
    clearTimeout(draft.timer);
    if (this.autosave())
      draft.timer = setTimeout(() => void this.save(draft.id), AUTOSAVE_DELAY_MS);
  }

  editRequest(id: string, mutate: (r: RequestData) => RequestData) {
    const draft = this.drafts.get(id);
    const req = draft?.request;
    if (!draft?.node || !req) return;
    draft.node = { ...draft.node, data: mutate(req) };
    this.changed(draft);
  }

  editFolder(id: string, mutate: (f: FolderNodeData) => FolderNodeData) {
    const draft = this.drafts.get(id);
    const folder = draft?.folder;
    if (!draft?.node || !folder) return;
    draft.node = { ...draft.node, data: mutate(folder) };
    this.changed(draft);
  }

  rename(id: string, name: string) {
    const draft = this.drafts.get(id);
    if (!draft?.node) return;
    draft.node = { ...draft.node, name };
    this.changed(draft);
  }

  /** Saves now (queued after any running save). */
  save(id: string): Promise<void> {
    const draft = this.drafts.get(id);
    if (!draft) return Promise.resolve();
    clearTimeout(draft.timer);
    draft.chain = draft.chain.then(async () => {
      if (!draft.dirty || !draft.node) return;
      const revision = draft.revision;
      const { name, data } = draft.node;
      draft.saving = true;
      try {
        const saved = await this.transport.call('node.update', { id: draft.id, name, data });
        if (draft.revision === revision) {
          draft.node = saved;
          draft.dirty = false;
        } else if (draft.node) {
          // Newer local edits stay; only adopt the server's version number.
          draft.node = { ...draft.node, version: saved.version, updatedAt: saved.updatedAt };
        }
      } catch (e) {
        toasts.error(errorMessage(e));
      } finally {
        draft.saving = false;
      }
    });
    return draft.chain;
  }

  /** Ensures pending edits are stored (before sending, closing, switching). */
  flush(id: string): Promise<void> {
    const draft = this.drafts.get(id);
    return draft?.dirty ? this.save(id) : (draft?.chain ?? Promise.resolve());
  }

  async flushAll() {
    await Promise.all([...this.drafts.keys()].map((id) => this.flush(id)));
  }

  /** Forgets drafts that are no longer open (after saving them). */
  async release(id: string) {
    await this.flush(id);
    this.drafts.delete(id);
  }

  destroy() {
    this.unsubscribe();
    for (const d of this.drafts.values()) clearTimeout(d.timer);
  }
}
