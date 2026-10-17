// Shared drag & drop state so items can be dragged between tree instances
// (e.g. from the request list onto the category tree).

import type { DropPosition } from './model';

export interface DropTarget {
  treeId: string;
  /** `null` = the tree root (dropped on empty space). */
  id: string | null;
  position: DropPosition;
}

export interface DropZone {
  /** Resolves the target under the pointer, or null if not droppable. */
  resolve(x: number, y: number, ids: string[]): DropTarget | null;
  drop(ids: string[], target: DropTarget): void;
}

class TreeDnd {
  dragging = $state<{ ids: string[]; label: string; source: string } | null>(null);
  pointer = $state({ x: 0, y: 0 });
  target = $state<DropTarget | null>(null);
  private zones = new Map<string, DropZone>();

  register(treeId: string, zone: DropZone) {
    this.zones.set(treeId, zone);
    return () => this.zones.delete(treeId);
  }

  start(source: string, ids: string[], label: string) {
    this.dragging = { ids, label, source };
  }

  move(x: number, y: number) {
    if (!this.dragging) return;
    this.pointer = { x, y };
    const el = document.elementFromPoint?.(x, y)?.closest<HTMLElement>('[data-tree-id]');
    const zone = el ? this.zones.get(el.dataset.treeId!) : undefined;
    this.target = zone?.resolve(x, y, this.dragging.ids) ?? null;
  }

  end(commit: boolean) {
    const { dragging, target } = this;
    this.dragging = null;
    this.target = null;
    if (commit && dragging && target) this.zones.get(target.treeId)?.drop(dragging.ids, target);
  }
}

export const treeDnd = new TreeDnd();
