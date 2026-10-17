import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import type { Transport } from '$lib/api/transport';
import AuthPanel from './AuthPanel.svelte';
import { defaults } from './auth/schemes';

describe('AuthPanel', () => {
  it('shows fields depending on other fields', () => {
    render(AuthPanel, { auth: defaults('oauth1'), onchange: vi.fn() });
    expect(screen.getByText('Consumer secret')).toBeInTheDocument();
    expect(screen.queryByText('Private key (PEM)')).not.toBeInTheDocument();
  });

  it('edits flags and numbers', async () => {
    const onchange = vi.fn();
    render(AuthPanel, { auth: defaults('asap'), onchange });
    await fireEvent.change(screen.getByRole('spinbutton', { name: 'Expires in (seconds)' }), {
      target: { value: '120' },
    });
    expect(onchange).toHaveBeenLastCalledWith(expect.objectContaining({ expiresInSecs: 120 }));
  });

  it('hides netrc outside the desktop app', () => {
    render(AuthPanel, { auth: defaults('netrc'), onchange: vi.fn() });
    expect(screen.getByText('Only available in the desktop app.')).toBeInTheDocument();
  });

  it('gets an OAuth 2.0 token with an automatic grant', async () => {
    const info = {
      status: 'valid',
      accessToken: 'at-1',
      idToken: '',
      hasRefreshToken: false,
      tokenType: 'Bearer',
      scope: 'read',
      expiresAt: null,
      obtainedAt: 1,
    };
    const call = vi.fn(async (name: string) =>
      name === 'oauth2.status' ? { ...info, status: 'none' } : info,
    );
    const transport = { kind: 'web', call, subscribe: () => () => {} } as unknown as Transport;
    const auth = { ...defaults('oauth2'), grant: 'clientCredentials' } as never;
    render(AuthPanel, { auth, nodeId: 'n1', transport, onchange: vi.fn() });
    await fireEvent.click(screen.getByRole('button', { name: 'Get token' }));
    expect(call).toHaveBeenCalledWith('oauth2.fetch', { nodeId: 'n1' });
    await waitFor(() => expect(screen.getByText('at-1')).toBeInTheDocument());
  });
});
