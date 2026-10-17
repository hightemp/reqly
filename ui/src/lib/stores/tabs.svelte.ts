// Open request tabs (Postman-style): a single preview tab that the next
// preview replaces, permanent and pinned tabs, and a reopen-closed stack.

import type { Transport } from '$lib/api/transport';

export interface Tab {
  id: string;
  pinned: boolean;
  /** Temporary tab, replaced by the next preview open. */
  preview: boolean;
}

interface Persisted {
  tabs: Tab[];
  activeId: string | null;
}

const CLOSED_LIMIT = 20;

export class TabsStore {
  tabs = $state<Tab[]>([]);
  activeId = $state<string | null>(null);
  private closed: string[] = [];
  private workspaceId: string | null = null;
  private saveTimer: ReturnType<typeof setTimeout> | undefined;

  constructor(private readonly transport: Transport) {}

  async load(workspaceId: string) {
    if (this.workspaceId && this.workspaceId !== workspaceId) await this.flush();
    this.workspaceId = workspaceId;
    this.closed = [];
    const { value } = await this.transport.call('settings.get', { key: `ui.tabs.${workspaceId}` });
    const state = (value ?? {}) as Partial<Persisted>;
    this.tabs = Array.isArray(state.tabs)
      ? state.tabs.filter((t) => typeof t?.id === 'string')
      : [];
    this.activeId = this.tabs.some((t) => t.id === state.activeId)
      ? state.activeId!
      : (this.tabs[0]?.id ?? null);
  }

  private changed() {
    clearTimeout(this.saveTimer);
    this.saveTimer = setTimeout(() => void this.flush(), 300);
  }

  async flush() {
    clearTimeout(this.saveTimer);
    if (!this.workspaceId) return;
    const value: Persisted = { tabs: this.tabs, activeId: this.activeId };
    await this.transport.call('settings.set', { key: `ui.tabs.${this.workspaceId}`, value });
  }

  private index(id: string) {
    return this.tabs.findIndex((t) => t.id === id);
  }

  /** Pinned tabs always stay on the left. */
  private normalize(tabs: Tab[]) {
    return [...tabs.filter((t) => t.pinned), ...tabs.filter((t) => !t.pinned)];
  }

  open(id: string, { preview = false }: { preview?: boolean } = {}) {
    const existing = this.index(id);
    if (existing >= 0) {
      if (!preview && this.tabs[existing]!.preview) this.makePermanent(id);
      this.activate(id);
      return;
    }
    const tab: Tab = { id, pinned: false, preview };
    const previewIndex = preview ? this.tabs.findIndex((t) => t.preview && !t.pinned) : -1;
    const next = [...this.tabs];
    if (previewIndex >= 0) {
      next[previewIndex] = tab;
    } else {
      const at = this.activeId ? this.index(this.activeId) + 1 : next.length;
      next.splice(at, 0, tab);
    }
    this.tabs = this.normalize(next);
    this.activate(id);
  }

  activate(id: string) {
    if (this.index(id) < 0) return;
    this.activeId = id;
    this.changed();
  }

  makePermanent(id: string) {
    this.tabs = this.tabs.map((t) => (t.id === id ? { ...t, preview: false } : t));
    this.changed();
  }

  togglePin(id: string) {
    this.tabs = this.normalize(
      this.tabs.map((t) => (t.id === id ? { ...t, pinned: !t.pinned, preview: false } : t)),
    );
    this.changed();
  }

  private remember(id: string) {
    this.closed = [...this.closed.filter((c) => c !== id), id].slice(-CLOSED_LIMIT);
  }

  close(id: string) {
    const i = this.index(id);
    if (i < 0) return;
    this.remember(id);
    const next = this.tabs.filter((t) => t.id !== id);
    if (this.activeId === id) this.activeId = (next[i] ?? next[i - 1])?.id ?? null;
    this.tabs = next;
    this.changed();
  }

  private closeWhere(keep: (t: Tab, i: number) => boolean) {
    const removed = this.tabs.filter((t, i) => !keep(t, i));
    removed.forEach((t) => this.remember(t.id));
    this.tabs = this.tabs.filter(keep);
    if (!this.tabs.some((t) => t.id === this.activeId))
      this.activeId = this.tabs.at(-1)?.id ?? null;
    this.changed();
  }

  closeOthers(id: string) {
    this.closeWhere((t) => t.pinned || t.id === id);
    this.activeId = id;
  }

  closeRight(id: string) {
    const i = this.index(id);
    this.closeWhere((t, j) => t.pinned || j <= i);
  }

  closeAll() {
    this.closeWhere((t) => t.pinned);
  }

  /** Reopens the most recently closed tab whose request still exists. */
  reopenClosed(exists: (id: string) => boolean): string | null {
    while (this.closed.length) {
      const id = this.closed.pop()!;
      if (exists(id) && this.index(id) < 0) {
        this.tabs = this.normalize([...this.tabs, { id, pinned: false, preview: false }]);
        this.activate(id);
        return id;
      }
    }
    return null;
  }

  next() {
    this.step(1);
  }

  prev() {
    this.step(-1);
  }

  private step(delta: number) {
    if (this.tabs.length === 0) return;
    const i = this.activeId ? this.index(this.activeId) : -1;
    this.activate(this.tabs[(i + delta + this.tabs.length) % this.tabs.length]!.id);
  }

  /** Ctrl+1…8 select by position, Ctrl+9 the last tab. */
  activateIndex(i: number) {
    const tab = i >= 8 ? this.tabs.at(-1) : this.tabs[i];
    if (tab) this.activate(tab.id);
  }

  move(from: number, to: number) {
    const next = [...this.tabs];
    const [tab] = next.splice(from, 1);
    if (!tab) return;
    next.splice(to, 0, tab);
    this.tabs = this.normalize(next);
    this.changed();
  }

  /** Drops tabs whose requests no longer exist. */
  retain(valid: Set<string>) {
    if (this.tabs.every((t) => valid.has(t.id))) return;
    const activeIndex = this.activeId ? this.index(this.activeId) : -1;
    const next = this.tabs.filter((t) => valid.has(t.id));
    if (this.activeId && !valid.has(this.activeId)) {
      const after = this.tabs.slice(activeIndex + 1).find((t) => valid.has(t.id));
      this.activeId = after?.id ?? next.at(-1)?.id ?? null;
    }
    this.tabs = next;
    this.changed();
  }

  destroy() {
    clearTimeout(this.saveTimer);
  }
}
