import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { tick } from 'svelte';
import { describe, expect, it, vi } from 'vitest';
import Harness from './EditorsHarness.test.svelte';

describe('Tabs', () => {
  it('switches with click and arrow keys', async () => {
    render(Harness, { kind: 'tabs' });
    expect(screen.getByRole('tab', { name: /Params/ })).toHaveAttribute('aria-selected', 'true');
    expect(screen.getByRole('tab', { name: /Headers/ })).toHaveTextContent('3');
    await fireEvent.click(screen.getByRole('tab', { name: /Body/ }));
    expect(screen.getByTestId('tab')).toHaveTextContent('body');
    await fireEvent.keyDown(screen.getByRole('tablist'), { key: 'ArrowRight' });
    expect(screen.getByTestId('tab')).toHaveTextContent('headers');
    await fireEvent.keyDown(screen.getByRole('tablist'), { key: 'Home' });
    expect(screen.getByTestId('tab')).toHaveTextContent('params');
  });
});

describe('KeyValueEditor', () => {
  it('edits, toggles and deletes rows', async () => {
    const onchange = vi.fn();
    render(Harness, { kind: 'kv', onchange });
    const names = screen.getAllByRole('textbox', { name: 'Name' });
    await fireEvent.input(names[0]!, { target: { value: 'Accept-Language' } });
    expect(onchange).toHaveBeenLastCalledWith([
      expect.objectContaining({ name: 'Accept-Language', value: '*/*' }),
    ]);
    await fireEvent.click(screen.getByRole('checkbox', { name: 'Enabled' }));
    expect(onchange).toHaveBeenLastCalledWith([expect.objectContaining({ enabled: false })]);
    await fireEvent.click(screen.getByRole('button', { name: 'Delete row' }));
    expect(onchange).toHaveBeenLastCalledWith([]);
  });

  it('adds a row when typing into the empty row', async () => {
    const onchange = vi.fn();
    render(Harness, { kind: 'kv', onchange });
    await fireEvent.input(screen.getByRole('textbox', { name: 'New name' }), {
      target: { value: 'X' },
    });
    expect(onchange).toHaveBeenLastCalledWith([
      expect.objectContaining({ name: 'Accept' }),
      expect.objectContaining({ name: 'X', value: '', enabled: true }),
    ]);
    expect(screen.getByRole('textbox', { name: 'New name' })).toHaveValue('');
  });

  it('switches to bulk mode and back', async () => {
    const onchange = vi.fn();
    render(Harness, { kind: 'kv', onchange });
    await fireEvent.click(screen.getByRole('button', { name: 'Bulk edit' }));
    const area = screen.getByRole('textbox', { name: 'Headers' });
    expect(area).toHaveValue('Accept: */*');
    await fireEvent.input(area, { target: { value: 'Accept: text/html\n// X-Debug: 1' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Table edit' }));
    expect(onchange).toHaveBeenLastCalledWith([
      expect.objectContaining({ name: 'Accept', value: 'text/html', enabled: true }),
      expect.objectContaining({ name: 'X-Debug', value: '1', enabled: false }),
    ]);
  });

  it('shows inherited rows read-only', () => {
    render(Harness, { kind: 'kv' });
    expect(screen.getByText('X-Team')).toBeInTheDocument();
    expect(screen.getByText('api folder')).toBeInTheDocument();
  });
});

describe('CodeEditor', () => {
  it('shows the value, reports edits and follows external updates', async () => {
    const onchange = vi.fn();
    const { component } = render(Harness, { kind: 'code', onchange });
    await waitFor(() => expect(document.querySelector('.cm-content')?.textContent).toBe('{"a":1}'));
    const view = (
      component as unknown as { editor(): { getView(): import('@codemirror/view').EditorView } }
    )
      .editor()
      .getView();
    view.dispatch({ changes: { from: 0, insert: ' ' } });
    expect(onchange).toHaveBeenCalledWith(' {"a":1}');
    await fireEvent.click(screen.getByTestId('set-external'));
    await tick();
    expect(view.state.doc.toString()).toBe('[]');
  });

  it('formats json', async () => {
    const { component } = render(Harness, { kind: 'code' });
    await waitFor(() => expect(document.querySelector('.cm-content')).toBeTruthy());
    const editor = (
      component as unknown as {
        editor(): { format(): boolean; getView(): import('@codemirror/view').EditorView };
      }
    ).editor();
    expect(editor.format()).toBe(true);
    expect(editor.getView().state.doc.toString()).toBe('{\n  "a": 1\n}');
  });

  it('is read-only when asked', async () => {
    render(Harness, { kind: 'code-readonly' });
    await waitFor(() =>
      expect(document.querySelector('.cm-content')).toHaveAttribute('contenteditable', 'false'),
    );
  });
});
