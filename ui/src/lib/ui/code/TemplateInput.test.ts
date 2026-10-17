import { render } from '@testing-library/svelte';
import { EditorView } from '@codemirror/view';
import { describe, expect, it, vi } from 'vitest';
import TemplateInput from './TemplateInput.svelte';

function view() {
  return EditorView.findFromDOM(document.querySelector('.cm-content') as HTMLElement)!;
}

describe('TemplateInput', () => {
  it('stays on one line and reports changes', () => {
    const onchange = vi.fn();
    render(TemplateInput, { value: 'a', label: 'URL', onchange });
    const v = view();
    v.dispatch({ changes: { from: 1, insert: 'b\nc' } });
    expect(v.state.doc.toString()).toBe('abc');
    expect(onchange).toHaveBeenLastCalledWith('abc');
  });

  it('runs onenter on Enter', () => {
    const onenter = vi.fn();
    render(TemplateInput, { value: '', label: 'URL', onenter });
    const content = document.querySelector('.cm-content') as HTMLElement;
    content.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
    expect(onenter).toHaveBeenCalled();
  });

  it('exposes an accessible single-line textbox', () => {
    render(TemplateInput, { value: '', label: 'URL' });
    const content = document.querySelector('.cm-content')!;
    expect(content).toHaveAttribute('aria-label', 'URL');
    expect(content).toHaveAttribute('aria-multiline', 'false');
  });
});
