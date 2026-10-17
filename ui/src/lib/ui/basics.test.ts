import { fireEvent, render, screen } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import Harness from './BasicsHarness.test.svelte';
import { methodColorVar, statusClass } from './badges';

describe('Button', () => {
  it('renders variants and handles clicks', async () => {
    const onclick = vi.fn();
    render(Harness, { kind: 'button', args: { variant: 'primary', onclick, label: 'Send' } });
    const btn = screen.getByRole('button', { name: 'Send' });
    expect(btn).toHaveClass('primary');
    await fireEvent.click(btn);
    expect(onclick).toHaveBeenCalledOnce();
  });

  it('shows a spinner and blocks clicks while loading', async () => {
    const onclick = vi.fn();
    render(Harness, { kind: 'button', args: { loading: true, onclick, label: 'Send' } });
    const btn = screen.getByRole('button', { name: 'Send' });
    expect(btn).toHaveAttribute('aria-busy', 'true');
    expect(screen.getByRole('progressbar')).toBeInTheDocument();
    await fireEvent.click(btn);
    expect(onclick).not.toHaveBeenCalled();
  });
});

describe('Input', () => {
  it('binds value and reports invalid state', async () => {
    const oninput = vi.fn();
    render(Harness, { kind: 'input', args: { value: 'a', invalid: true, oninput, label: 'Name' } });
    const input = screen.getByRole('textbox', { name: 'Name' });
    expect(input).toHaveAttribute('aria-invalid', 'true');
    await fireEvent.input(input, { target: { value: 'ab' } });
    expect(oninput).toHaveBeenCalled();
    expect(screen.getByTestId('value')).toHaveTextContent('ab');
  });
});

describe('Checkbox and Switch', () => {
  it('toggle with click and keyboard', async () => {
    render(Harness, { kind: 'checkbox', args: { checked: false, label: 'Enabled' } });
    const box = screen.getByRole('checkbox', { name: 'Enabled' });
    expect(box).toHaveAttribute('aria-checked', 'false');
    await fireEvent.click(box);
    expect(box).toHaveAttribute('aria-checked', 'true');
    await fireEvent.keyDown(box, { key: ' ' });
    expect(box).toHaveAttribute('aria-checked', 'false');
  });

  it('switch has switch role', async () => {
    render(Harness, { kind: 'switch', args: { checked: true, label: 'Autosave' } });
    const sw = screen.getByRole('switch', { name: 'Autosave' });
    expect(sw).toHaveAttribute('aria-checked', 'true');
    await fireEvent.keyDown(sw, { key: 'Enter' });
    expect(sw).toHaveAttribute('aria-checked', 'false');
  });

  it('disabled controls do not toggle', async () => {
    render(Harness, { kind: 'checkbox', args: { checked: false, disabled: true, label: 'X' } });
    const box = screen.getByRole('checkbox', { name: 'X' });
    await fireEvent.click(box);
    expect(box).toHaveAttribute('aria-checked', 'false');
  });
});

describe('badges', () => {
  it('maps methods to color tokens', () => {
    expect(methodColorVar('get')).toBe('var(--c-method-get)');
    expect(methodColorVar('PURGE')).toBe('var(--c-method-other)');
  });

  it('classifies statuses', () => {
    expect(statusClass(204)).toBe('2xx');
    expect(statusClass(404)).toBe('4xx');
    expect(statusClass(null)).toBe('error');
    expect(statusClass(99)).toBe('error');
  });

  it('renders method and status badges', () => {
    render(Harness, { kind: 'badges', args: {} });
    expect(screen.getByText('DEL')).toBeInTheDocument();
    expect(screen.getByText('201 Created')).toBeInTheDocument();
  });
});

describe('ProgressBar', () => {
  it('is determinate with a value and indeterminate without', () => {
    render(Harness, { kind: 'progress', args: { value: 0.25 } });
    const bars = screen.getAllByRole('progressbar');
    expect(bars[0]).toHaveAttribute('aria-valuenow', '25');
    expect(bars[1]).not.toHaveAttribute('aria-valuenow');
  });
});
