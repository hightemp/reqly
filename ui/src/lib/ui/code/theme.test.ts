import { EditorSelection, EditorState } from '@codemirror/state';
import { EditorView } from '@codemirror/view';
import { describe, expect, it } from 'vitest';
import { activeLine, editability } from './theme';

describe('current line highlight', () => {
  it('steps aside while text is selected so the selection shows', () => {
    const view = new EditorView({
      state: EditorState.create({ doc: 'one\ntwo', extensions: [activeLine()] }),
      parent: document.body,
    });
    expect(view.dom.classList).not.toContain('cm-hasSelection');
    view.dispatch({ selection: EditorSelection.range(0, 5) });
    expect(view.dom.classList).toContain('cm-hasSelection');
    view.dispatch({ selection: EditorSelection.cursor(2) });
    expect(view.dom.classList).not.toContain('cm-hasSelection');
    view.destroy();
  });
});

describe('read-only editors', () => {
  it('take the focus so Ctrl+A and Ctrl+C work in them', () => {
    const view = new EditorView({
      state: EditorState.create({ doc: '{"a": 1}', extensions: [editability(true)] }),
      parent: document.body,
    });
    expect(view.state.readOnly).toBe(true);
    expect(view.contentDOM.getAttribute('contenteditable')).toBe('false');
    expect(view.contentDOM.tabIndex).toBe(0);
    view.contentDOM.focus();
    expect(document.activeElement).toBe(view.contentDOM);
    view.destroy();

    const editable = new EditorView({
      state: EditorState.create({ doc: '', extensions: [editability(false)] }),
      parent: document.body,
    });
    expect(editable.state.readOnly).toBe(false);
    expect(editable.contentDOM.getAttribute('contenteditable')).toBe('true');
    editable.destroy();
  });
});
