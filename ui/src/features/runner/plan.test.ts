import { describe, expect, it } from 'vitest';
import type { RequestTreeItem } from '$lib/stores/tree.svelte';
import { runnableRequests } from './plan';

const node = (
  id: string,
  parentId: string | null,
  extra: Partial<RequestTreeItem> = {},
): RequestTreeItem => ({
  id,
  parentId,
  isFolder: false,
  label: id,
  method: 'GET',
  protocol: 'http',
  url: null,
  createdAt: 0,
  updatedAt: 0,
  ...extra,
});

describe('runnableRequests', () => {
  const items = [
    node('a', null),
    node('F', null, { isFolder: true, method: null, protocol: null }),
    node('G', 'F', { isFolder: true, method: null, protocol: null }),
    node('b', 'F'),
    node('c', 'G', { protocol: 'graphql', method: 'POST' }),
    node('ws', null, { protocol: 'webSocket', method: 'WS' }),
    node('sse', 'F', { protocol: 'sse' }),
    node('d', null),
  ];

  it('lists requests depth first in display order with their folders', () => {
    expect(runnableRequests(items, null)).toEqual([
      { id: 'a', name: 'a', method: 'GET', path: [] },
      { id: 'c', name: 'c', method: 'POST', path: ['F', 'G'] },
      { id: 'b', name: 'b', method: 'GET', path: ['F'] },
      { id: 'd', name: 'd', method: 'GET', path: [] },
    ]);
  });

  it('starts at a folder', () => {
    expect(runnableRequests(items, 'F').map((r) => r.id)).toEqual(['c', 'b']);
  });
});
