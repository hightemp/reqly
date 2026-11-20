import { EditorSelection, EditorState } from '@codemirror/state';
import { EditorView } from '@codemirror/view';
import { describe, expect, it } from 'vitest';
import { activeLine } from './theme';

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
