// The context menu for text: Cut / Copy / Paste / Select all in text fields
// and code editors, Copy / Select all over read-only text. The app's
// webview shows no menu of its own (and a browser's would be out of place).

import { EditorView } from '@codemirror/view';
import { detectPlatform, formatCombo, type Platform } from '$lib/hotkeys/hotkeys';
import { t } from '$lib/i18n';
import { readClipboardText } from '$lib/platform/clipboard';
import { isItem, menus, type MenuEntry } from './overlay/menus.svelte';

/** Text the menu works on: a field, an editor or plain selectable text. */
interface Surface {
  editable: boolean;
  /** Fields and editors keep their selection while the menu has the focus. */
  keepsSelection: boolean;
  selected(): string;
  /** Replaces the selection (editable surfaces only). */
  replace(text: string): void;
  selectAll(): void;
  /** Gives the keyboard back to the field or editor after a menu action. */
  focus(): void;
  /** Where a `paste` event goes, so the app's own paste handlers see it. */
  pasteTarget: HTMLElement;
}

type Field = HTMLInputElement | HTMLTextAreaElement;

const TEXT_INPUTS = new Set(['text', 'search', 'url', 'email', 'tel', 'password', 'number', '']);

function fieldSurface(el: Field): Surface {
  const range = () => {
    try {
      return { start: el.selectionStart, end: el.selectionEnd };
    } catch {
      return { start: null, end: null };
    }
  };
  return {
    editable: !el.readOnly && !el.disabled,
    keepsSelection: true,
    selected() {
      const { start, end } = range();
      return start === null || end === null ? '' : el.value.slice(start, end);
    },
    replace(text) {
      const { start, end } = range();
      if (start === null || end === null) el.value += text;
      else el.setRangeText(text, start, end, 'end');
      el.dispatchEvent(new Event('input', { bubbles: true }));
    },
    selectAll: () => el.select(),
    focus: () => el.focus(),
    pasteTarget: el,
  };
}

function editorSurface(view: EditorView): Surface {
  return {
    editable: !view.state.readOnly,
    keepsSelection: true,
    selected: () =>
      view.state.selection.ranges
        .filter((r) => !r.empty)
        .map((r) => view.state.sliceDoc(r.from, r.to))
        .join('\n'),
    replace: (text) =>
      view.dispatch(view.state.replaceSelection(text), { userEvent: 'input.paste' }),
    selectAll: () => view.dispatch({ selection: { anchor: 0, head: view.state.doc.length } }),
    focus: () => view.focus(),
    pasteTarget: view.contentDOM,
  };
}

function textSurface(root: Element): Surface {
  return {
    editable: false,
    keepsSelection: false,
    selected: () => window.getSelection()?.toString() ?? '',
    replace: () => {},
    selectAll: () => {
      const range = document.createRange();
      range.selectNodeContents(root);
      const selection = window.getSelection();
      selection?.removeAllRanges();
      selection?.addRange(range);
    },
    focus: () => {},
    pasteTarget: root as HTMLElement,
  };
}

/** The text surface under `target`, if there is one. */
export function surfaceAt(target: EventTarget | null): Surface | null {
  if (!(target instanceof Element)) return null;
  const editor = target.closest<HTMLElement>('.cm-editor');
  if (editor) {
    const view = EditorView.findFromDOM(editor);
    if (view) return editorSurface(view);
  }
  const field = target.closest('input, textarea');
  if (field instanceof HTMLTextAreaElement) return fieldSurface(field);
  if (field instanceof HTMLInputElement) {
    return TEXT_INPUTS.has(field.type) ? fieldSurface(field) : null;
  }
  const selectable = target.closest<HTMLElement>('.selectable');
  if (selectable) return textSurface(selectable);
  // Selected text elsewhere (e.g. inside a modal that allows selection).
  const selection = window.getSelection();
  if (selection && !selection.isCollapsed && selection.toString()) {
    if (selection.containsNode(target, true)) return textSurface(target);
  }
  return null;
}

async function paste(s: Surface) {
  const read = await readClipboardText();
  if (read.text === null) return;
  // Through a `paste` event first: the URL bar imports a pasted cURL.
  let event: Event | null = null;
  try {
    const data = new DataTransfer();
    data.setData('text/plain', read.text);
    event = new ClipboardEvent('paste', { clipboardData: data, bubbles: true, cancelable: true });
  } catch {
    // No DataTransfer constructor here: insert directly.
  }
  if (event && !s.pasteTarget.dispatchEvent(event)) return;
  s.replace(read.text);
}

/** Menu entries for a text surface. */
export function textMenuItems(s: Surface, platform: Platform = detectPlatform()): MenuEntry[] {
  const k = (combo: string) => formatCombo(combo, platform);
  const selected = s.selected();
  const copy = () => void navigator.clipboard?.writeText(selected);
  const entries: MenuEntry[] = [];
  if (s.editable) {
    entries.push({
      id: 'text-cut',
      label: t('actions.cut'),
      shortcut: k('Mod+X'),
      disabled: !selected,
      onSelect: () => {
        copy();
        s.replace('');
      },
    });
  }
  entries.push({
    id: 'text-copy',
    label: t('actions.copy'),
    shortcut: k('Mod+C'),
    disabled: !selected,
    onSelect: copy,
  });
  if (s.editable) {
    entries.push({
      id: 'text-paste',
      label: t('actions.paste'),
      shortcut: k('Mod+V'),
      onSelect: () => void paste(s),
    });
  }
  entries.push(
    { separator: true },
    {
      id: 'text-select-all',
      label: t('actions.selectAll'),
      shortcut: k('Mod+A'),
      onSelect: () => s.selectAll(),
    },
  );
  // Each action gives the keyboard back to the text it worked on.
  for (const e of entries) {
    if (!isItem(e) || !e.onSelect) continue;
    const run = e.onSelect;
    e.onSelect = () => {
      s.focus();
      run();
    };
  }
  return entries;
}

/**
 * The document's `contextmenu` handler: the text menu over text, no menu
 * elsewhere (menus of rows and tabs stop the event before it gets here).
 * On touch screens text keeps the system's selection toolbar.
 */
export function onTextContextMenu(e: MouseEvent, touch: boolean) {
  if (e.defaultPrevented) return;
  const surface = surfaceAt(e.target);
  if (touch && surface) return;
  e.preventDefault();
  if (!surface) return;
  if (!surface.keepsSelection) keepSelection();
  menus.openAt({ x: e.clientX, y: e.clientY }, textMenuItems(surface));
}

/**
 * WebKit (the Linux and macOS apps) drops the selection of read-only text
 * when the menu takes the focus: put it back once the menu has it.
 */
function keepSelection() {
  const selection = window.getSelection();
  if (!selection || selection.isCollapsed) return;
  const ranges = Array.from({ length: selection.rangeCount }, (_, i) =>
    selection.getRangeAt(i).cloneRange(),
  );
  const restore = () => {
    if (!selection.isCollapsed) return;
    selection.removeAllRanges();
    for (const r of ranges) selection.addRange(r);
  };
  const onFocus = () => {
    document.removeEventListener('focusin', onFocus);
    restore();
    requestAnimationFrame(restore);
  };
  document.addEventListener('focusin', onFocus);
  // Nothing took the focus: stop listening.
  setTimeout(() => document.removeEventListener('focusin', onFocus), 500);
}
