import type { RequestTreeItem } from '$lib/stores/tree.svelte';

export const ALL_ID = '__all__';

/**
 * Requests inside `folderId` (or everywhere when null), depth-first. Grouped
 * mode adds a header item per subfolder labelled with its path relative to
 * the category; requests directly in the category come first.
 */
export function buildRequestList(
  items: RequestTreeItem[],
  folderId: string | null,
  flat: boolean,
  filter?: (item: RequestTreeItem) => boolean,
): RequestTreeItem[] {
  const children = new Map<string | null, RequestTreeItem[]>();
  for (const item of items) {
    const list = children.get(item.parentId) ?? [];
    list.push(item);
    children.set(item.parentId, list);
  }
  const out: RequestTreeItem[] = [];
  const walk = (parent: string | null, path: string[]) => {
    const kids = children.get(parent) ?? [];
    const requests = kids.filter((k) => !k.isFolder && (!filter || filter(k)));
    if (requests.length) {
      const groupId = path.length && !flat ? `group:${parent}` : null;
      if (groupId) {
        out.push({
          id: groupId,
          parentId: null,
          isFolder: true,
          label: path.join(' / '),
          method: null,
          url: null,
          createdAt: 0,
          updatedAt: 0,
        });
      }
      out.push(...requests.map((r) => ({ ...r, parentId: groupId })));
    }
    for (const folder of kids.filter((k) => k.isFolder)) walk(folder.id, [...path, folder.label]);
  };
  walk(folderId, []);
  return out;
}
