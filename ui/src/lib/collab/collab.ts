// Collaborative editing of text fields on a multi-user server (ADR-012).
// Each bound editor gets a Yjs document joined to the server's copy; local
// edits and cursors go out as commands, other people's arrive as events.

import { getContext, setContext } from 'svelte';
import * as Y from 'yjs';
import {
  Awareness,
  applyAwarenessUpdate,
  encodeAwarenessUpdate,
  removeAwarenessStates,
} from 'y-protocols/awareness';
import type { Transport } from '$lib/api/transport';
import { ApiError, TRANSPORT_TOPICS } from '$lib/api/transport';

/** Transaction origin of changes that came from the server. */
const REMOTE = Symbol('remote');

export interface CollabUser {
  name: string;
  /** Stable per user, to tell cursors apart. */
  color: string;
}

export interface CollabBinding {
  ytext: Y.Text;
  awareness: Awareness;
  undoManager: Y.UndoManager;
  destroy(): void;
}

export function toBase64(bytes: Uint8Array): string {
  let s = '';
  for (const b of bytes) s += String.fromCharCode(b);
  return btoa(s);
}

export function fromBase64(text: string): Uint8Array {
  const s = atob(text);
  const out = new Uint8Array(s.length);
  for (let i = 0; i < s.length; i++) out[i] = s.charCodeAt(i);
  return out;
}

const COLORS = [
  '#e5484d',
  '#f76b15',
  '#ffc53d',
  '#46a758',
  '#12a594',
  '#0090ff',
  '#8e4ec6',
  '#d6409f',
];

export function userColor(id: string): string {
  let h = 0;
  for (const c of id) h = (h * 31 + c.charCodeAt(0)) >>> 0;
  return COLORS[h % COLORS.length]!;
}

interface UpdateEvent {
  nodeId: string;
  field: string;
  docId: string;
  update: string;
  clientId: number;
}

export class CollabService {
  /** Fields bound per node: the editor store does not save them itself. */
  private readonly bound = new Map<string, Map<string, number>>();

  constructor(
    private readonly transport: Transport,
    private readonly user: () => CollabUser | null,
  ) {}

  get enabled(): boolean {
    return this.user() !== null;
  }

  /** Paths of `nodeId` that are bound to a shared document. */
  paths(nodeId: string): string[] {
    return [...(this.bound.get(nodeId)?.keys() ?? [])];
  }

  private mark(nodeId: string, field: string, delta: 1 | -1) {
    const fields = this.bound.get(nodeId) ?? new Map<string, number>();
    const n = (fields.get(field) ?? 0) + delta;
    if (n > 0) fields.set(field, n);
    else fields.delete(field);
    if (fields.size) this.bound.set(nodeId, fields);
    else this.bound.delete(nodeId);
  }

  /**
   * Joins the shared document of a field. `onStale` runs when the server
   * forgot it (restart, idle) or the connection came back: bind again.
   */
  async bind(nodeId: string, field: string, onStale: () => void): Promise<CollabBinding> {
    const user = this.user();
    const joined = await this.transport.call('collab.join', { nodeId, field });
    const doc = new Y.Doc();
    const ytext = doc.getText('t');
    Y.applyUpdate(doc, fromBase64(joined.update), REMOTE);
    const docId = joined.docId;
    let alive = true;
    const stale = () => {
      if (!alive) return;
      binding.destroy();
      onStale();
    };

    // Sends local edits one after another, in order.
    let queue: Promise<unknown> = Promise.resolve();
    const onUpdate = (update: Uint8Array, origin: unknown) => {
      if (origin === REMOTE) return;
      queue = queue.then(() =>
        this.transport
          .call('collab.update', {
            nodeId,
            field,
            docId,
            update: toBase64(update),
            clientId: doc.clientID,
          })
          .catch((e: unknown) => {
            if (ApiError.from(e).code === 'collab.stale') stale();
            else console.warn('[collab] update failed', e);
          }),
      );
    };
    doc.on('update', onUpdate);

    const awareness = new Awareness(doc);
    const onAwareness = (
      { added, updated, removed }: { added: number[]; updated: number[]; removed: number[] },
      origin: unknown,
    ) => {
      if (origin === REMOTE) return;
      const update = encodeAwarenessUpdate(awareness, [...added, ...updated, ...removed]);
      void this.transport
        .call('collab.awareness', {
          nodeId,
          field,
          update: toBase64(update),
          clientId: doc.clientID,
        })
        .catch(() => {});
    };
    awareness.on('update', onAwareness);

    const off = this.transport.subscribe((e) => {
      if (e.topic === TRANSPORT_TOPICS.reconnected) return stale();
      if (e.topic !== 'collab.update' && e.topic !== 'collab.awareness') return;
      const p = e.payload as UpdateEvent;
      if (p.nodeId !== nodeId || p.field !== field || p.clientId === doc.clientID) return;
      if (e.topic === 'collab.awareness') {
        const known = awareness.getStates().has(p.clientId);
        applyAwarenessUpdate(awareness, fromBase64(p.update), REMOTE);
        // Someone new: tell them where we are.
        if (!known && awareness.getLocalState()) awareness.setLocalState(awareness.getLocalState());
      } else if (p.docId === docId) {
        Y.applyUpdate(doc, fromBase64(p.update), REMOTE);
      }
    });

    // Announce ourselves once others can answer.
    if (user) awareness.setLocalStateField('user', { ...user, colorLight: `${user.color}33` });

    const undoManager = new Y.UndoManager(ytext);
    this.mark(nodeId, field, 1);
    const binding: CollabBinding = {
      ytext,
      awareness,
      undoManager,
      destroy: () => {
        if (!alive) return;
        alive = false;
        this.mark(nodeId, field, -1);
        removeAwarenessStates(awareness, [doc.clientID], 'local');
        off();
        awareness.off('update', onAwareness);
        doc.off('update', onUpdate);
        undoManager.destroy();
        awareness.destroy();
        doc.destroy();
      },
    };
    return binding;
  }
}

const KEY = Symbol('collab');

export function provideCollab(service: CollabService) {
  setContext(KEY, service);
}

export function collabService(): CollabService | undefined {
  return getContext<CollabService | undefined>(KEY);
}
