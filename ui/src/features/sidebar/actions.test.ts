import { describe, expect, it } from 'vitest';
import type { MenuEntry } from '$lib/ui/overlay/menus.svelte';
import { readOnly } from './actions';

const item = (id: string): MenuEntry => ({ id, label: id, onSelect: () => {} });

describe('readOnly', () => {
  it('keeps only entries that change nothing', () => {
    const entries: MenuEntry[] = [
      { heading: 'Create' },
      item('new-http'),
      item('new-folder'),
      { separator: true },
      item('rename'),
      item('duplicate'),
      item('copy'),
      item('paste'),
      item('copy-as'),
      item('export'),
      { separator: true },
      item('sort'),
      { separator: true },
      item('delete'),
    ];
    const ids = readOnly(entries).map((e) => ('id' in e ? e.id : 'separator' in e ? '|' : '#'));
    expect(ids).toEqual(['copy', 'copy-as', 'export']);
  });
});
