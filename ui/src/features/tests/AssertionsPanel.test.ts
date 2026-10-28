import { fireEvent, render, screen } from '@testing-library/svelte';
import { tick } from 'svelte';
import { describe, expect, it, vi } from 'vitest';
import type { Assertion } from '$lib/api/generated/Assertion';
import AssertionsPanel from './AssertionsPanel.svelte';

const row = (patch: Partial<Assertion> = {}): Assertion => ({
  id: 'a1',
  enabled: true,
  source: 'status',
  property: '',
  operator: 'equals',
  value: '200',
  ...patch,
});

async function choose(name: string, option: string) {
  await fireEvent.click(screen.getByRole('combobox', { name }));
  await tick();
  await fireEvent.click(screen.getByRole('option', { name: option }));
}

describe('AssertionsPanel', () => {
  it('explains itself when empty and adds a status check', async () => {
    const onchange = vi.fn();
    render(AssertionsPanel, { assertions: [], onchange });
    expect(screen.getByText(/Check the response without code/)).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Add assertion' }));
    expect(onchange).toHaveBeenCalledWith([
      expect.objectContaining({ source: 'status', operator: 'equals', value: '200' }),
    ]);
  });

  it('shows the property only for headers and expressions', () => {
    render(AssertionsPanel, {
      assertions: [row(), row({ id: 'a2', source: 'jsonPath', property: '$.id' })],
      onchange: vi.fn(),
    });
    const properties = document.querySelectorAll('.cm-content[aria-label="Property"]');
    expect(properties).toHaveLength(1);
  });

  it('changes the source and operator', async () => {
    const onchange = vi.fn();
    render(AssertionsPanel, { assertions: [row()], onchange });
    await choose('Source', 'Header');
    expect(onchange).toHaveBeenLastCalledWith([expect.objectContaining({ source: 'header' })]);
    await choose('Operator', 'exists');
    expect(onchange).toHaveBeenLastCalledWith([expect.objectContaining({ operator: 'exists' })]);
  });

  it('hides the value for exists and edits a schema in a code editor', () => {
    const { unmount } = render(AssertionsPanel, {
      assertions: [row({ operator: 'exists' })],
      onchange: vi.fn(),
    });
    expect(document.querySelector('[aria-label="Expected value"]')).toBeNull();
    unmount();
    render(AssertionsPanel, {
      assertions: [row({ source: 'body', operator: 'matchesSchema', value: '{}' })],
      onchange: vi.fn(),
    });
    expect(document.querySelector('[aria-label="JSON Schema"]')).not.toBeNull();
  });

  it('toggles and deletes rows', async () => {
    const onchange = vi.fn();
    render(AssertionsPanel, { assertions: [row()], onchange });
    await fireEvent.click(screen.getByRole('checkbox', { name: 'Enabled' }));
    expect(onchange).toHaveBeenLastCalledWith([expect.objectContaining({ enabled: false })]);
    await fireEvent.click(screen.getByRole('button', { name: 'Delete assertion' }));
    expect(onchange).toHaveBeenLastCalledWith([]);
  });
});
