import { describe, expect, it } from 'vitest';
import type { RequestTreeItem } from '$lib/stores/tree.svelte';
import { buildRequestList } from './categories';

const f = (id: string, parentId: string | null, label = id): RequestTreeItem => ({
  id,
  parentId,
  isFolder: true,
  label,
  method: null,
  url: null,
  createdAt: 0,
  updatedAt: 0,
});
const r = (id: string, parentId: string | null): RequestTreeItem => ({
  id,
  parentId,
  isFolder: false,
  label: id,
  method: 'GET',
  url: `http://${id}`,
  createdAt: 0,
  updatedAt: 0,
});

// api/{list, users/{get, admin/{del}}}, root-req
const items = [
  f('api', null),
  r('list', 'api'),
  f('users', 'api'),
  r('get', 'users'),
  f('admin', 'users'),
  r('del', 'admin'),
  r('root-req', null),
];

describe('buildRequestList', () => {
  it('groups requests of the category recursively by relative path', () => {
    const list = buildRequestList(items, 'api', false);
    expect(list.map((i) => `${i.isFolder ? 'G' : 'R'}:${i.label}:${i.parentId ?? '-'}`)).toEqual([
      'R:list:-',
      'G:users:-',
      'R:get:group:users',
      'G:users / admin:-',
      'R:del:group:admin',
    ]);
  });

  it('flat mode lists all requests without groups', () => {
    // Requests directly in the category come first.
    expect(buildRequestList(items, null, true).map((i) => i.id)).toEqual([
      'root-req',
      'list',
      'get',
      'del',
    ]);
  });

  it('applies the search filter', () => {
    expect(
      buildRequestList(items, null, true, (i) => i.id.startsWith('d')).map((i) => i.id),
    ).toEqual(['del']);
  });

  it('all requests (null) starts at the root', () => {
    const list = buildRequestList(items, null, false);
    expect(list[0]).toMatchObject({ id: 'root-req', isFolder: false });
    expect(list.filter((i) => i.isFolder).map((i) => i.label)).toEqual([
      'api',
      'api / users',
      'api / users / admin',
    ]);
  });
});
