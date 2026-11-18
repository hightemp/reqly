import { describe, expect, it, vi } from 'vitest';
import type { Services } from '$lib/services.svelte';
import { EditorStore } from '$lib/stores/editor.svelte';
import { TabsStore } from '$lib/stores/tabs.svelte';
import { FakeTransport } from '$lib/testing/fakeTransport';
import { trackTabs } from './info.svelte';

function node(id: string, version = 1, deletedAt: number | null = null) {
  return {
    id,
    version,
    collectionId: 'c1',
    parentId: null,
    name: id,
    sortKey: 'V',
    createdAt: 0,
    updatedAt: 0,
    deletedAt,
    data: { kind: 'request', method: 'GET', url: 'http://a' },
  };
}

function setup() {
  const t = new FakeTransport();
  const stored = new Map([
    ['r1', node('r1')],
    ['r2', node('r2')],
  ]);
  t.handle('node.get', ((input: { id: string }) => {
    const found = stored.get(input.id);
    if (!found) throw new Error('not found');
    return found;
  }) as never);
  t.handle('settings.set', (() => ({})) as never);
  const editor = new EditorStore(t, () => true);
  const tabs = new TabsStore(t);
  const services = { editor, tabs } as unknown as Services;
  const stop = $effect.root(() => trackTabs(services));
  return { t, stored, tabs, editor, stop };
}

describe('trackTabs', () => {
  it('closes the tab of a request deleted while it is open', async () => {
    const { t, stored, tabs, editor, stop } = setup();
    tabs.open('r1');
    tabs.open('r2');
    await vi.waitFor(() => expect(editor.get('r1')).toBeDefined());

    stored.set('r1', node('r1', 2, 1000));
    t.emit({ topic: 'tree.changed', payload: { collectionIds: ['c1'], nodeIds: ['r1'] } });

    await vi.waitFor(() => expect(tabs.tabs.map((x) => x.id)).toEqual(['r2']));
    expect(editor.get('r1')).toBeUndefined();
    stop();
  });

  it('closes the tab of a request that can no longer be loaded', async () => {
    const { tabs, stop } = setup();
    tabs.open('gone');
    await vi.waitFor(() => expect(tabs.tabs).toEqual([]));
    stop();
  });
});
