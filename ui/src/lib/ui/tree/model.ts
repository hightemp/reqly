// Pure tree logic: flattening for rendering, drop rules and selection helpers.

export interface TreeItem {
  id: string;
  parentId: string | null;
  isFolder: boolean;
  label: string;
}

export interface TreeRow<T extends TreeItem = TreeItem> {
  id: string;
  item: T;
  depth: number;
  hasChildren: boolean;
  expanded: boolean;
}

export type DropPosition = 'before' | 'inside' | 'after';

function childrenMap<T extends TreeItem>(items: T[]): Map<string | null, T[]> {
  const ids = new Set(items.map((i) => i.id));
  const map = new Map<string | null, T[]>();
  for (const item of items) {
    const parent = item.parentId !== null && ids.has(item.parentId) ? item.parentId : null;
    const list = map.get(parent) ?? [];
    list.push(item);
    map.set(parent, list);
  }
  return map;
}

/**
 * Visible rows in display order. With `filter`, only matches and their
 * ancestors are shown and ancestors are expanded regardless of `expanded`.
 */
export function flatten<T extends TreeItem>(
  items: T[],
  expanded: ReadonlySet<string>,
  filter?: (item: T) => boolean,
): TreeRow<T>[] {
  const children = childrenMap(items);
  let keep: Set<string> | null = null;
  if (filter) {
    keep = new Set();
    const byId = new Map(items.map((i) => [i.id, i]));
    for (const item of items) {
      if (!filter(item)) continue;
      let cur: T | undefined = item;
      while (cur && !keep.has(cur.id)) {
        keep.add(cur.id);
        cur = cur.parentId ? byId.get(cur.parentId) : undefined;
      }
    }
  }

  const rows: TreeRow<T>[] = [];
  const walk = (parent: string | null, depth: number) => {
    for (const item of children.get(parent) ?? []) {
      if (keep && !keep.has(item.id)) continue;
      const kids = children.get(item.id) ?? [];
      const isOpen =
        item.isFolder && (keep ? kids.some((k) => keep!.has(k.id)) : expanded.has(item.id));
      rows.push({ id: item.id, item, depth, hasChildren: kids.length > 0, expanded: isOpen });
      if (isOpen) walk(item.id, depth + 1);
    }
  };
  walk(null, 0);
  return rows;
}

/** Drop zone from the pointer's vertical position within a row (0..1). */
export function dropPosition(ratio: number, isFolder: boolean): DropPosition {
  if (isFolder) {
    if (ratio < 0.25) return 'before';
    if (ratio > 0.75) return 'after';
    return 'inside';
  }
  return ratio < 0.5 ? 'before' : 'after';
}

function ancestorsOf(items: TreeItem[], id: string): Set<string> {
  const byId = new Map(items.map((i) => [i.id, i]));
  const out = new Set<string>();
  let cur = byId.get(id)?.parentId ?? null;
  while (cur !== null && !out.has(cur)) {
    out.add(cur);
    cur = byId.get(cur)?.parentId ?? null;
  }
  return out;
}

export function canDrop(
  items: TreeItem[],
  dragged: string[],
  targetId: string,
  position: DropPosition,
): boolean {
  const target = items.find((i) => i.id === targetId);
  if (!target) return false;
  if (position === 'inside' && !target.isFolder) return false;
  if (dragged.includes(targetId)) return false;
  // The target (or its new parent) must not be inside a dragged folder.
  const ancestors = ancestorsOf(items, targetId);
  return !dragged.some((d) => ancestors.has(d));
}

/** Ids between `anchor` and `target` (inclusive) in visible order. */
export function rangeSelect(visible: string[], anchor: string, target: string): string[] {
  const a = visible.indexOf(anchor);
  const b = visible.indexOf(target);
  if (a < 0 || b < 0) return [target];
  const [from, to] = a < b ? [a, b] : [b, a];
  return visible.slice(from, to + 1);
}

/** Removes ids whose ancestor is also selected (order preserved). */
export function normalizeSelection(items: TreeItem[], ids: string[]): string[] {
  const selected = new Set(ids);
  return ids.filter((id) => ![...ancestorsOf(items, id)].some((a) => selected.has(a)));
}

/** Next index after `from` whose label starts with `prefix` (wrapping), or -1. */
export function typeaheadIndex(labels: string[], from: number, prefix: string): number {
  const p = prefix.toLowerCase();
  for (let n = 1; n <= labels.length; n++) {
    const i = (from + n) % labels.length;
    if (labels[i]!.toLowerCase().startsWith(p)) return i;
  }
  return -1;
}
