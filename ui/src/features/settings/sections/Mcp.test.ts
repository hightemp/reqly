import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { McpConfig } from '$lib/api/generated/McpConfig';
import { capabilities } from '$lib/platform/capabilities.svelte';
import { McpStore } from '$lib/stores/mcp.svelte';
import { FakeTransport } from '$lib/testing/fakeTransport';
import Mcp from './Mcp.svelte';

const TOOLS = [
  { name: 'get_tree', group: 'read' as const },
  { name: 'delete_item', group: 'edit' as const },
  { name: 'send_request', group: 'run' as const },
];

function setup(listen: { endpoint: string | null; error: string | null }) {
  const t = new FakeTransport();
  let config: McpConfig = { enabled: false, port: 7655, tools: {} };
  let hasToken = false;
  const status = (token?: string) => ({
    available: true,
    config,
    tools: TOOLS.map((x) => ({
      ...x,
      enabled: config.tools[x.name] ?? x.group !== 'edit',
    })),
    hasToken,
    listen: config.enabled ? listen : { endpoint: null, error: null },
    ...(token ? { token } : {}),
  });
  t.handle('mcp.config', () => status());
  t.handle('mcp.log', () => [
    {
      at: 0,
      tool: 'send_request',
      arguments: '{"id":"r1"}',
      ok: false,
      error: 'http.connect',
      durationMs: 12,
    },
  ]);
  t.handle('mcp.setConfig', (input) => {
    config = input;
    const token = input.enabled && !hasToken ? 'tok-123' : undefined;
    if (token) hasToken = true;
    return status(token);
  });
  t.handle('mcp.regenerateToken', () => ({ token: 'tok-456' }));
  const mcp = new McpStore(t);
  return { t, mcp, config: () => config };
}

describe('MCP settings', () => {
  beforeEach(() => {
    capabilities.set('desktop', []);
    Object.assign(navigator, { clipboard: { writeText: vi.fn(async () => {}) } });
  });

  it('turns MCP on, shows the address, the token once and client configs', async () => {
    const { mcp } = setup({ endpoint: 'http://127.0.0.1:7655/mcp', error: null });
    render(Mcp, { mcp });
    await screen.findByText('send_request');
    expect(screen.getByText('Not running')).toBeTruthy();
    expect(screen.getByText('Failed · 12 ms')).toBeTruthy();

    await fireEvent.click(screen.getByRole('switch', { name: 'MCP server' }));
    await screen.findByText('http://127.0.0.1:7655/mcp');
    expect(screen.getByText('tok-123')).toBeTruthy();
    expect(screen.getByText(/claude mcp add --transport http reqly/).textContent).toContain(
      'Bearer tok-123',
    );
    await fireEvent.click(screen.getByRole('button', { name: 'Codex' }));
    expect(screen.getByText(/\[mcp_servers\.reqly\]/).textContent).toContain(
      'url = "http://127.0.0.1:7655/mcp"',
    );
    await fireEvent.click(screen.getByRole('button', { name: 'Cursor' }));
    expect(screen.getByText(/mcpServers/)).toBeTruthy();

    await fireEvent.click(screen.getByRole('button', { name: 'Hide' }));
    expect(screen.queryByText('tok-123')).toBeNull();
    expect(screen.getByText('Set (shown only once)')).toBeTruthy();
    expect(screen.getByText(/mcpServers/).textContent).toContain('<token>');
  });

  it('switches groups and single tools', async () => {
    const { mcp, config } = setup({ endpoint: null, error: null });
    render(Mcp, { mcp });
    const edit = await screen.findByRole('checkbox', { name: 'Edit requests' });
    expect(edit.getAttribute('aria-checked')).toBe('false');
    await fireEvent.click(edit);
    await waitFor(() => expect(config().tools).toEqual({ delete_item: true }));

    await fireEvent.click(screen.getByRole('button', { name: 'Show tools of “Read”' }));
    await fireEvent.click(screen.getByRole('checkbox', { name: 'get_tree' }));
    await waitFor(() => expect(config().tools.get_tree).toBe(false));

    await fireEvent.click(screen.getByRole('button', { name: 'Read only' }));
    await waitFor(() =>
      expect(config().tools).toEqual({ get_tree: true, delete_item: false, send_request: false }),
    );
  });

  it('reports a busy port and validates the port field', async () => {
    const { mcp, t } = setup({ endpoint: null, error: '127.0.0.1:7655: address in use' });
    render(Mcp, { mcp });
    await fireEvent.click(await screen.findByRole('switch', { name: 'MCP server' }));
    expect((await screen.findByRole('alert')).textContent).toContain('address in use');

    const port = screen.getByRole('textbox', { name: 'Port' });
    await fireEvent.input(port, { target: { value: '80' } });
    await fireEvent.blur(port);
    const saves = () => t.calls.filter((c) => c.name === 'mcp.setConfig').length;
    expect(saves()).toBe(1);
    await fireEvent.input(port, { target: { value: '8800' } });
    await fireEvent.keyDown(port, { key: 'Enter' });
    await waitFor(() => expect(saves()).toBe(2));
  });

  it('multi-user servers point to personal API tokens and hide the port', async () => {
    capabilities.set('server', []);
    const { mcp } = setup({ endpoint: '/mcp', error: null });
    render(Mcp, { mcp, multiUser: true, origin: 'https://reqly.example.com' });
    await fireEvent.click(await screen.findByRole('switch', { name: 'MCP server' }));
    await screen.findByText('https://reqly.example.com/mcp');
    expect(screen.queryByRole('textbox', { name: 'Port' })).toBeNull();
    expect(screen.queryByText('tok-123')).toBeNull();
    expect(screen.getByText(/claude mcp add/).textContent).toContain('<API token>');
  });
});
