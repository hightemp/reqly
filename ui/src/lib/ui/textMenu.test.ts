import { EditorState } from '@codemirror/state';
import { EditorView } from '@codemirror/view';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { menus, isItem, type MenuItem } from './overlay/menus.svelte';
import { templateHostFacet } from '$lib/template/codemirror';
import type { TemplateHost } from '$lib/template/host';
import { onTextContextMenu, surfaceAt, textMenuItems } from './textMenu';

vi.mock('$lib/platform/clipboard', () => ({
  readClipboardText: vi.fn(async () => ({ text: 'pasted' })),
}));

const writeText = vi.fn(async (_: string) => {});

beforeEach(() => {
  Object.defineProperty(navigator, 'clipboard', { value: { writeText }, configurable: true });
});

afterEach(() => {
  document.body.innerHTML = '';
  menus.close(false);
  writeText.mockClear();
});

function labels(entries: ReturnType<typeof textMenuItems>) {
  return entries.filter(isItem).map((e) => `${e.label}${e.disabled ? ' (off)' : ''}`);
}

function item(entries: ReturnType<typeof textMenuItems>, id: string): MenuItem {
  return entries.filter(isItem).find((e) => e.id === id)!;
}

function rightClick(target: Element) {
  const e = new MouseEvent('contextmenu', { bubbles: true, cancelable: true });
  Object.defineProperty(e, 'target', { value: target });
  return e;
}

describe('text context menu', () => {
  it('cuts, copies and pastes in a text field', async () => {
    const area = document.createElement('textarea');
    document.body.append(area);
    area.value = 'hello world';
    area.setSelectionRange(0, 5);
    const input = vi.fn();
    area.addEventListener('input', input);

    const entries = textMenuItems(surfaceAt(area)!, 'linux');
    expect(labels(entries)).toEqual(['Cut', 'Copy', 'Paste', 'Select all']);
    expect(item(entries, 'text-copy').shortcut).toBe('Ctrl+C');

    area.blur();
    item(entries, 'text-cut').onSelect!();
    expect(document.activeElement).toBe(area);
    expect(writeText).toHaveBeenCalledWith('hello');
    expect(area.value).toBe(' world');
    expect(input).toHaveBeenCalled();

    area.setSelectionRange(6, 6);
    item(textMenuItems(surfaceAt(area)!), 'text-paste').onSelect!();
    await vi.waitFor(() => expect(area.value).toBe(' worldpasted'));
  });

  it('turns Cut and Copy off without a selection', () => {
    const input = document.createElement('input');
    document.body.append(input);
    input.value = 'abc';
    input.setSelectionRange(1, 1);
    expect(labels(textMenuItems(surfaceAt(input)!))).toEqual([
      'Cut (off)',
      'Copy (off)',
      'Paste',
      'Select all',
    ]);
  });

  it('copies from a read-only code editor', () => {
    const view = new EditorView({
      state: EditorState.create({
        doc: '{"a": 1}',
        selection: { anchor: 1, head: 4 },
        extensions: [EditorState.readOnly.of(true)],
      }),
      parent: document.body,
    });
    const line = view.contentDOM.querySelector('.cm-line')!;
    const entries = textMenuItems(surfaceAt(line)!);
    expect(labels(entries)).toEqual(['Copy', 'Select all']);
    item(entries, 'text-copy').onSelect!();
    expect(writeText).toHaveBeenCalledWith('"a"');
    item(entries, 'text-select-all').onSelect!();
    expect(view.state.selection.main.to).toBe(8);
    view.destroy();
  });

  it('pastes into an editable code editor', async () => {
    const view = new EditorView({
      state: EditorState.create({ doc: 'ab', selection: { anchor: 1 } }),
      parent: document.body,
    });
    item(textMenuItems(surfaceAt(view.contentDOM)!), 'text-paste').onSelect!();
    await vi.waitFor(() => expect(view.state.doc.toString()).toBe('apastedb'));
    view.destroy();
  });

  it('inserts a template tag through the tag editor', () => {
    const editTag = vi.fn((_text: string, apply: (next: string) => void) =>
      apply("{% uuid 'v4' %}"),
    );
    const host: TemplateHost = {
      load: async () => ({ variables: [], functions: [], faker: [], dynamicVariables: [] }),
      onChange: () => () => {},
      preview: async (t) => t,
      editTag,
    };
    const view = new EditorView({
      state: EditorState.create({
        doc: 'id=',
        selection: { anchor: 3 },
        extensions: [templateHostFacet.of(host)],
      }),
      parent: document.body,
    });
    const entries = textMenuItems(surfaceAt(view.contentDOM)!);
    item(entries, 'text-insert-tag').onSelect!();
    expect(editTag).toHaveBeenCalledWith('', expect.any(Function));
    expect(view.state.doc.toString()).toBe("id={% uuid 'v4' %}");
    view.destroy();

    // Read-only editors and editors without templates have no such item.
    const plain = new EditorView({ state: EditorState.create({ doc: '' }), parent: document.body });
    expect(
      textMenuItems(surfaceAt(plain.contentDOM)!).some(
        (e) => 'id' in e && e.id === 'text-insert-tag',
      ),
    ).toBe(false);
    plain.destroy();
  });

  it('selects all of a selectable block', () => {
    document.body.innerHTML = '<div class="selectable"><p>one</p><p id="two">two</p></div>';
    const entries = textMenuItems(surfaceAt(document.getElementById('two')!)!);
    expect(labels(entries)).toEqual(['Copy (off)', 'Select all']);
    item(entries, 'text-select-all').onSelect!();
    expect(window.getSelection()!.toString()).toBe('onetwo');
  });

  it('opens over text and shows nothing elsewhere', () => {
    document.body.innerHTML = '<div id="plain">x</div><p class="selectable" id="text">y</p>';
    const plain = rightClick(document.getElementById('plain')!);
    onTextContextMenu(plain, false);
    expect(plain.defaultPrevented).toBe(true);
    expect(menus.current).toBeNull();

    const text = rightClick(document.getElementById('text')!);
    onTextContextMenu(text, false);
    expect(text.defaultPrevented).toBe(true);
    expect(menus.current?.items.filter(isItem).map((e) => e.id)).toEqual([
      'text-copy',
      'text-select-all',
    ]);
  });

  it('copies the parts of a table row', () => {
    document.body.innerHTML =
      '<table class="selectable"><tr data-copy-name="Content-Type" data-copy-value="text/plain" data-copy-row="Content-Type: text/plain"><td id="v">text/plain</td></tr></table>';
    onTextContextMenu(rightClick(document.getElementById('v')!), false);
    const items = menus.current!.items.filter(isItem);
    expect(items.map((e) => e.label)).toEqual([
      'Copy',
      'Select all',
      'Copy name',
      'Copy value',
      'Copy row',
    ]);
    items.find((e) => e.id === 'text-copy-row')!.onSelect!();
    expect(writeText).toHaveBeenCalledWith('Content-Type: text/plain');
    items.find((e) => e.id === 'text-copy-name')!.onSelect!();
    expect(writeText).toHaveBeenLastCalledWith('Content-Type');
  });

  it('leaves menus of rows and the touch text toolbar alone', () => {
    document.body.innerHTML = '<p class="selectable" id="text">y</p>';
    const handled = rightClick(document.getElementById('text')!);
    handled.preventDefault();
    onTextContextMenu(handled, false);
    expect(menus.current).toBeNull();

    const touch = rightClick(document.getElementById('text')!);
    onTextContextMenu(touch, true);
    expect(touch.defaultPrevented).toBe(false);
    expect(menus.current).toBeNull();
  });

  it('puts back a read-only selection that the menu focus dropped (WebKit)', () => {
    document.body.innerHTML = '<p class="selectable" id="text">some answer</p>';
    const p = document.getElementById('text')!;
    const range = document.createRange();
    range.selectNodeContents(p);
    window.getSelection()!.removeAllRanges();
    window.getSelection()!.addRange(range);

    onTextContextMenu(rightClick(p), false);
    // WebKit clears the selection when a menu item takes the focus.
    window.getSelection()!.removeAllRanges();
    const button = document.createElement('button');
    document.body.append(button);
    button.focus();
    button.dispatchEvent(new FocusEvent('focusin', { bubbles: true }));
    expect(window.getSelection()!.toString()).toBe('some answer');
  });
});
