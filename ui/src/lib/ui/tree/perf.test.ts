import { describe, expect, it } from 'vitest';
import { flatten, type TreeItem } from './model';

// PRD §8: a 10 000-node tree must stay smooth. Flattening runs on every
// expand/filter change, so it has to be well below one frame budget.
function bigTree(folders: number, perFolder: number): TreeItem[] {
  const items: TreeItem[] = [];
  for (let f = 0; f < folders; f++) {
    items.push({
      id: `f${f}`,
      parentId: f % 10 === 0 ? null : `f${f - (f % 10)}`,
      isFolder: true,
      label: `folder ${f}`,
    });
    for (let r = 0; r < perFolder; r++)
      items.push({
        id: `r${f}-${r}`,
        parentId: `f${f}`,
        isFolder: false,
        label: `request ${f}-${r}`,
      });
  }
  return items;
}

describe('tree performance', () => {
  it('flattens 10k nodes quickly', () => {
    const items = bigTree(500, 19);
    expect(items.length).toBe(10_000);
    const expanded = new Set(items.filter((i) => i.isFolder).map((i) => i.id));
    const start = performance.now();
    const rows = flatten(items, expanded);
    const filtered = flatten(items, expanded, (i) => i.label.includes('7-1'));
    const ms = performance.now() - start;
    expect(rows.length).toBe(10_000);
    expect(filtered.length).toBeGreaterThan(0);
    expect(ms).toBeLessThan(200);
  });
});
