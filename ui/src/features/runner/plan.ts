import type { RequestTreeItem } from '$lib/stores/tree.svelte';

export interface PlannedRequest {
  id: string;
  name: string;
  method: string;
  /** Folder names from the collection root, outermost first. */
  path: string[];
}

/** Requests under `rootId` (the collection when `null`) that a run can send, depth first. */
export function runnableRequests(
  items: RequestTreeItem[],
  rootId: string | null,
): PlannedRequest[] {
  const children = new Map<string | null, RequestTreeItem[]>();
  for (const item of items)
    children.set(item.parentId, [...(children.get(item.parentId) ?? []), item]);
  const out: PlannedRequest[] = [];
  const visit = (parent: string | null, path: string[]) => {
    for (const item of children.get(parent) ?? []) {
      if (item.isFolder) visit(item.id, [...path, item.label]);
      else if (item.protocol === 'http' || item.protocol === 'graphql' || item.protocol == null)
        out.push({ id: item.id, name: item.label, method: item.method ?? 'GET', path });
    }
  };
  const rootPath: string[] = [];
  for (let p = rootId; p; p = items.find((i) => i.id === p)?.parentId ?? null)
    rootPath.unshift(items.find((i) => i.id === p)?.label ?? '');
  visit(rootId, rootPath);
  return out;
}
