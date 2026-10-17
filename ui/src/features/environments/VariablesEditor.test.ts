import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import VariablesEditor from './VariablesEditor.svelte';

describe('VariablesEditor', () => {
  it('adds a variable from the empty row and toggles secrets', async () => {
    const onchange = vi.fn();
    render(VariablesEditor, {
      data: { host: 'x' },
      secrets: [],
      allowSecrets: true,
      label: 'Vars',
      onchange,
    });
    await fireEvent.input(screen.getByRole('textbox', { name: 'New variable name' }), {
      target: { value: 'token' },
    });
    expect(onchange).toHaveBeenLastCalledWith({ host: 'x', token: '' }, []);
    const lock = screen.getAllByRole('button', { name: 'Mark as secret' });
    await fireEvent.click(lock[1]!);
    expect(onchange).toHaveBeenLastCalledWith({ host: 'x', token: '' }, ['token']);
  });

  it('edits as JSON and rejects non-objects', async () => {
    const onchange = vi.fn();
    render(VariablesEditor, { data: { a: 1 }, label: 'Vars', onchange });
    await fireEvent.click(screen.getByRole('button', { name: 'Edit as JSON' }));
    await waitFor(() =>
      expect(document.querySelector('.cm-content')?.textContent).toContain('"a": 1'),
    );
    const view = (await import('@codemirror/view')).EditorView.findFromDOM(
      document.querySelector('.cm-content') as HTMLElement,
    )!;
    view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: '[1]' } });
    expect(await screen.findByRole('alert')).toHaveTextContent('Variables must be a JSON object');
    view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: '{"b": true}' } });
    expect(onchange).toHaveBeenLastCalledWith({ b: true }, []);
  });
});
