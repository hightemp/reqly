import { render, screen } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { RequestSettings } from '$lib/api/generated/RequestSettings';
import { capabilities } from '$lib/platform/capabilities.svelte';
import SettingsPanel from './SettingsPanel.svelte';

const settings = {} as RequestSettings;

describe('SettingsPanel', () => {
  beforeEach(() => {
    (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {};
  });
  afterEach(() => {
    delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
    capabilities.set('desktop', []);
  });

  it('offers Unix sockets and interfaces in the desktop app', () => {
    capabilities.set('desktop', []);
    render(SettingsPanel, { settings, onchange: vi.fn() });
    expect(screen.getByRole('textbox', { name: 'Unix domain socket' })).toBeEnabled();
    expect(screen.getByRole('textbox', { name: 'Network interface' })).toBeEnabled();
  });

  it('explains what the platform cannot do', () => {
    capabilities.set('mobile', ['unixSocket', 'interface', 'osTag']);
    render(SettingsPanel, { settings, onchange: vi.fn() });
    expect(screen.getByRole('textbox', { name: 'Unix domain socket' })).toBeDisabled();
    expect(screen.getByRole('textbox', { name: 'Network interface' })).toBeDisabled();
    expect(screen.getAllByText('Not available on Android')).toHaveLength(2);
  });
});
