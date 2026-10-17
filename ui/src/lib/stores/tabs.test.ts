import { describe, expect, it } from 'vitest';
import { FakeTransport } from '$lib/testing/fakeTransport';
import { TabsStore } from './tabs.svelte';

const ids = (s: TabsStore) =>
  s.tabs.map((t) => `${t.id}${t.preview ? '*' : ''}${t.pinned ? '^' : ''}`);

async function setup() {
  const t = new FakeTransport();
  const settings = t.withSettings();
  const s = new TabsStore(t);
  await s.load('w1');
  return { s, t, settings };
}

describe('TabsStore', () => {
  it('preview tabs are replaced by the next preview', async () => {
    const { s } = await setup();
    s.open('a', { preview: true });
    s.open('b', { preview: true });
    expect(ids(s)).toEqual(['b*']);
    expect(s.activeId).toBe('b');
  });

  it('permanent open or edit keeps the tab', async () => {
    const { s } = await setup();
    s.open('a', { preview: true });
    s.makePermanent('a');
    s.open('b', { preview: true });
    s.open('b');
    s.open('c', { preview: true });
    expect(ids(s)).toEqual(['a', 'b', 'c*']);
  });

  it('opens new tabs after the active one', async () => {
    const { s } = await setup();
    s.open('a');
    s.open('b');
    s.activate('a');
    s.open('c');
    expect(ids(s)).toEqual(['a', 'c', 'b']);
  });

  it('close activates the right neighbor, then the left', async () => {
    const { s } = await setup();
    ['a', 'b', 'c'].forEach((id) => s.open(id));
    s.activate('b');
    s.close('b');
    expect(s.activeId).toBe('c');
    s.close('c');
    expect(s.activeId).toBe('a');
    s.close('a');
    expect(s.activeId).toBeNull();
  });

  it('reopens closed tabs in reverse order, skipping missing requests', async () => {
    const { s } = await setup();
    ['a', 'b', 'c'].forEach((id) => s.open(id));
    s.close('a');
    s.close('b');
    expect(s.reopenClosed((id) => id !== 'b')).toBe('a');
    expect(ids(s)).toEqual(['c', 'a']);
  });

  it('close others / right / all keep pinned tabs', async () => {
    const { s } = await setup();
    ['a', 'b', 'c', 'd'].forEach((id) => s.open(id));
    s.togglePin('d');
    expect(ids(s)).toEqual(['d^', 'a', 'b', 'c']);
    s.closeRight('a');
    expect(ids(s)).toEqual(['d^', 'a']);
    s.open('e');
    s.closeOthers('e');
    expect(ids(s)).toEqual(['d^', 'e']);
    s.closeAll();
    expect(ids(s)).toEqual(['d^']);
  });

  it('cycles and jumps by index', async () => {
    const { s } = await setup();
    ['a', 'b', 'c'].forEach((id) => s.open(id));
    s.next();
    expect(s.activeId).toBe('a');
    s.prev();
    expect(s.activeId).toBe('c');
    s.activateIndex(1);
    expect(s.activeId).toBe('b');
    s.activateIndex(8); // Ctrl+9 = last
    expect(s.activeId).toBe('c');
  });

  it('moves tabs by drag', async () => {
    const { s } = await setup();
    ['a', 'b', 'c'].forEach((id) => s.open(id));
    s.move(0, 2);
    expect(ids(s)).toEqual(['b', 'c', 'a']);
  });

  it('drops tabs of deleted requests', async () => {
    const { s } = await setup();
    ['a', 'b', 'c'].forEach((id) => s.open(id));
    s.activate('b');
    s.retain(new Set(['a', 'c']));
    expect(ids(s)).toEqual(['a', 'c']);
    expect(s.activeId).toBe('c');
  });

  it('persists per workspace', async () => {
    const { s, t, settings } = await setup();
    s.open('a');
    s.open('b', { preview: true });
    await s.flush();
    expect(settings.get('ui.tabs.w1')).toEqual({
      tabs: [
        { id: 'a', pinned: false, preview: false },
        { id: 'b', pinned: false, preview: true },
      ],
      activeId: 'b',
    });
    const restored = new TabsStore(t);
    await restored.load('w1');
    expect(ids(restored)).toEqual(['a', 'b*']);
    await restored.load('w2');
    expect(ids(restored)).toEqual([]);
  });
});
