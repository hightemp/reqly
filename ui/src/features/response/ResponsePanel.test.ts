import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import { createServices } from '$lib/services.svelte';
import { Viewport } from '$lib/stores/viewport.svelte';
import { FakeTransport } from '$lib/testing/fakeTransport';
import ResponsePanel from './ResponsePanel.svelte';

const media = { matches: false, addEventListener() {}, removeEventListener() {} };
const meta = { createdAt: 0, updatedAt: 0, version: 1, deletedAt: null, sortKey: 'V' };

function node(verifyTls: boolean | null) {
  return {
    id: 'r1',
    collectionId: 'c1',
    parentId: null,
    name: 'Self-signed',
    ...meta,
    data: {
      kind: 'request',
      method: 'GET',
      url: 'https://self-signed.test',
      params: [],
      pathParams: [],
      headers: [],
      body: { type: 'none' },
      auth: { type: 'none' },
      settings: { verifyTls },
      variables: [],
      scripts: { preRequest: '', afterResponse: '' },
      assertions: [],
      description: '',
    },
  };
}

const failed = {
  id: 'resp1',
  requestId: 'r1',
  createdAt: 0,
  status: null,
  statusText: '',
  httpVersion: '',
  headers: [],
  bodyHash: null,
  bodySize: 0,
  headersSize: 0,
  contentType: null,
  timings: {},
  timeline: [],
  requestSnapshot: { method: 'GET', url: 'https://self-signed.test', headers: [] },
  redirects: 0,
  finalUrl: '',
  remoteAddr: null,
  error: {
    code: 'http.tls',
    message: 'SSL peer certificate was not OK',
    params: { detail: 'self-signed certificate' },
  },
  scriptLogs: [],
  tests: [],
  assertions: [],
};

async function setup(verifyTls: boolean | null) {
  const t = new FakeTransport();
  t.withSettings();
  t.withEmptyWorkspace();
  let current = node(verifyTls);
  t.handle('node.get', () => current as never);
  t.handle('node.update', (input) => {
    // Only the changed fields of the request data.
    const { fields } = input as unknown as { fields: Record<string, unknown> };
    current = { ...current, data: { ...current.data, ...fields } as typeof current.data };
    return current as never;
  });
  t.handle('response.list', () => [{ id: 'resp1' }] as never);
  t.handle('response.get', () => failed as never);
  t.handle('request.send', () => failed as never);
  const services = await createServices(t, {
    platform: 'linux',
    media,
    root: document.createElement('div'),
    viewport: new Viewport({ compact: media }),
  });
  services.tabs.open('r1');
  await services.editor.open('r1');
  const onsettings = vi.fn();
  render(ResponsePanel, { services, onsettings });
  return { t, onsettings, current: () => current };
}

describe('response panel, certificate errors', () => {
  it('offers to send without checking the certificate', async () => {
    const { t, onsettings, current } = await setup(null);
    await screen.findByRole('alert');
    expect(screen.getByText(/certificate/i, { selector: 'p' })).toBeTruthy();
    await fireEvent.click(
      screen.getByRole('button', { name: 'Send without checking the certificate' }),
    );
    await waitFor(() => expect(t.calls.some((c) => c.name === 'request.send')).toBe(true));
    expect(current().data.settings.verifyTls).toBe(false);

    await fireEvent.click(screen.getByRole('button', { name: 'Certificate settings' }));
    expect(onsettings).toHaveBeenCalledWith('http');
  });

  it('does not offer it when the check is already off', async () => {
    await setup(false);
    await screen.findByRole('alert');
    expect(
      screen.queryByRole('button', { name: 'Send without checking the certificate' }),
    ).toBeNull();
  });
});
