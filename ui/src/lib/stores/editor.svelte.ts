// Request/folder drafts with autosave. Saves for one node are serialized so
// the server always ends with the latest local state.

import type { FolderData } from '$lib/api/generated/FolderData';
import type { GrpcRequest } from '$lib/api/generated/GrpcRequest';
import type { HttpRequest } from '$lib/api/generated/HttpRequest';
import type { Node } from '$lib/api/generated/Node';
import type { NodeData } from '$lib/api/generated/NodeData';
import type { WebSocketRequest } from '$lib/api/generated/WebSocketRequest';
import type { Transport } from '$lib/api/transport';
import { SvelteMap } from 'svelte/reactivity';
import { errorMessage } from '$lib/errors';
import { t } from '$lib/i18n';
import { toasts } from '$lib/ui/overlay/toasts.svelte';

const AUTOSAVE_DELAY_MS = 500;

type RequestData = HttpRequest & { kind: 'request' };
type FolderNodeData = FolderData & { kind: 'folder' };
export type WebSocketData = WebSocketRequest & { kind: 'webSocket' };
export type GrpcData = GrpcRequest & { kind: 'grpc' };

/** Same JSON value (field values are plain data). */
const same = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b);

/** Top-level fields of `data` (and whether the name) changed from `base`. */
export function changes(base: Node, node: Node) {
  const before = base.data as Record<string, unknown>;
  const after = node.data as Record<string, unknown>;
  const fields: Record<string, unknown> = {};
  const keys = [...Object.keys(before), ...Object.keys(after)];
  for (const key of keys.filter((k, i) => keys.indexOf(k) === i)) {
    if (key !== 'kind' && !same(before[key], after[key])) fields[key] = after[key];
  }
  return { fields, name: node.name !== base.name ? node.name : undefined };
}

/** `fresh` from the server with the local edits (made since `base`) on top. */
export function rebase(local: Node, base: Node, fresh: Node): Node {
  const { fields, name } = changes(base, local);
  return {
    ...fresh,
    name: name ?? fresh.name,
    data: { ...fresh.data, ...fields } as NodeData,
  };
}

export class Draft {
  node = $state<Node | null>(null);
  /** The last version from the server; local edits are relative to it. */
  base: Node | null = null;
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

  get webSocket(): WebSocketData | null {
    return this.node?.data.kind === 'webSocket' ? (this.node.data as WebSocketData) : null;
  }

  get grpc(): GrpcData | null {
    return this.node?.data.kind === 'grpc' ? (this.node.data as GrpcData) : null;
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
    /** Viewers may try edits locally; nothing is saved. */
    readonly readonly: () => boolean = () => false,
  ) {
    this.unsubscribe = transport.subscribe((e) => {
      if (e.topic === 'node.overwritten') {
        const p = e.payload as { name: string; fields: string[]; by: string };
        toasts.show({
          message: t('editor.overwritten', { by: p.by, name: p.name, fields: p.fields.join(', ') }),
          kind: 'info',
        });
        return;
      }
      if (e.topic !== 'tree.changed') return;
      const ids = (e.payload as { nodeIds?: string[] } | null)?.nodeIds ?? [];
      for (const id of ids) {
        const draft = this.drafts.get(id);
        if (draft && !draft.saving) void this.refresh(draft);
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
      draft.base = draft.node;
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

  /** Takes others' changes; unsaved local edits stay on top. */
  private async refresh(draft: Draft) {
    try {
      const fresh = await this.transport.call('node.get', { id: draft.id });
      if (draft.saving || !draft.node || !draft.base || fresh.version <= draft.base.version) return;
      draft.node = draft.dirty ? rebase(draft.node, draft.base, fresh) : fresh;
      draft.base = fresh;
    } catch {
      // Deleted nodes are cleaned up by the tabs store.
    }
  }

  private changed(draft: Draft) {
    draft.dirty = true;
    draft.revision += 1;
    clearTimeout(draft.timer);
    if (this.autosave() && !this.readonly())
      draft.timer = setTimeout(() => void this.save(draft.id), AUTOSAVE_DELAY_MS);
  }

  editRequest(id: string, mutate: (r: RequestData) => RequestData) {
    const draft = this.drafts.get(id);
    const req = draft?.request;
    if (!draft?.node || !req) return;
    draft.node = { ...draft.node, data: mutate(req) };
    this.changed(draft);
  }

  /** Edits node data of the same kind (WebSocket, gRPC…). */
  editData<D extends NodeData>(id: string, kind: D['kind'], mutate: (d: D) => D) {
    const draft = this.drafts.get(id);
    if (!draft?.node || draft.node.data.kind !== kind) return;
    draft.node = { ...draft.node, data: mutate(draft.node.data as D) };
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
      if (!draft.dirty || !draft.node || !draft.base || this.readonly()) return;
      const revision = draft.revision;
      const base = draft.base;
      const { fields, name } = changes(base, draft.node);
      draft.saving = true;
      try {
        // Only the changed fields: others' edits to other fields survive.
        const saved = await this.transport.call('node.update', {
          id: draft.id,
          baseVersion: base.version,
          fields,
          ...(name !== undefined ? { name } : {}),
        });
        if (draft.revision === revision) {
          draft.node = saved;
          draft.dirty = false;
        } else if (draft.node) {
          // Newer local edits stay on top of the saved version.
          draft.node = rebase(draft.node, base, saved);
        }
        draft.base = saved;
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
