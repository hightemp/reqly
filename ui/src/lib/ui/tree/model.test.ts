import { describe, expect, it } from 'vitest';
import {
  canDrop,
  dropPosition,
  flatten,
  normalizeSelection,
  rangeSelect,
  typeaheadIndex,
  type TreeItem,
} from './model';

// root
// ├─ api (folder)
// │  ├─ users (folder)
// │  │  └─ list
// │  └─ health
// └─ readme
const items: TreeItem[] = [
  { id: 'api', parentId: null, isFolder: true, label: 'api' },
  { id: 'users', parentId: 'api', isFolder: true, label: 'users' },
  { id: 'list', parentId: 'users', isFolder: false, label: 'list' },
  { id: 'health', parentId: 'api', isFolder: false, label: 'health' },
  { id: 'readme', parentId: null, isFolder: false, label: 'readme' },
];

describe('flatten', () => {
  it('shows only children of expanded folders, with depth', () => {
    const rows = flatten(items, new Set(['api']));
    expect(rows.map((r) => `${r.depth}:${r.id}`)).toEqual([
      '0:api',
      '1:users',
      '1:health',
      '0:readme',
    ]);
    expect(rows[0]).toMatchObject({ expanded: true, hasChildren: true });
    expect(rows[1]).toMatchObject({ expanded: false, hasChildren: true });
  });

  it('keeps input order within each parent', () => {
    const rows = flatten(items, new Set(['api', 'users']));
    expect(rows.map((r) => r.id)).toEqual(['api', 'users', 'list', 'health', 'readme']);
  });

  it('with a filter shows matches and their ancestors, expanded', () => {
    const rows = flatten(items, new Set(), (i) => i.label.includes('lis'));
    expect(rows.map((r) => r.id)).toEqual(['api', 'users', 'list']);
    expect(rows.every((r) => r.id === 'list' || r.expanded)).toBe(true);
  });

  it('treats orphans as roots instead of dropping them', () => {
    const rows = flatten([{ id: 'x', parentId: 'gone', isFolder: false, label: 'x' }], new Set());
    expect(rows.map((r) => r.id)).toEqual(['x']);
  });
});

describe('dropPosition', () => {
  it('splits folders in three zones and leaves in two', () => {
    expect(dropPosition(0.1, true)).toBe('before');
    expect(dropPosition(0.5, true)).toBe('inside');
    expect(dropPosition(0.9, true)).toBe('after');
    expect(dropPosition(0.4, false)).toBe('before');
    expect(dropPosition(0.6, false)).toBe('after');
  });
});

describe('canDrop', () => {
  it('forbids dropping into itself or a descendant', () => {
    expect(canDrop(items, ['api'], 'users', 'inside')).toBe(false);
    expect(canDrop(items, ['api'], 'list', 'before')).toBe(false);
    expect(canDrop(items, ['api'], 'api', 'after')).toBe(false);
    expect(canDrop(items, ['users'], 'readme', 'after')).toBe(true);
    expect(canDrop(items, ['list'], 'api', 'inside')).toBe(true);
  });

  it('only folders accept inside', () => {
    expect(canDrop(items, ['health'], 'readme', 'inside')).toBe(false);
  });
});

describe('selection helpers', () => {
  const visible = ['api', 'users', 'list', 'health', 'readme'];

  it('range-selects between anchor and target in visible order', () => {
    expect(rangeSelect(visible, 'list', 'api')).toEqual(['api', 'users', 'list']);
    expect(rangeSelect(visible, 'users', 'readme')).toEqual(['users', 'list', 'health', 'readme']);
  });

  it('drops children of selected folders', () => {
    expect(normalizeSelection(items, ['list', 'api', 'readme'])).toEqual(['api', 'readme']);
  });

  it('typeahead cycles from the current row', () => {
    const labels = ['api', 'auth', 'users', 'apps'];
    expect(typeaheadIndex(labels, 0, 'a')).toBe(1);
    expect(typeaheadIndex(labels, 1, 'a')).toBe(3);
    expect(typeaheadIndex(labels, 3, 'a')).toBe(0);
    expect(typeaheadIndex(labels, 0, 'us')).toBe(2);
    expect(typeaheadIndex(labels, 0, 'zz')).toBe(-1);
  });
});
