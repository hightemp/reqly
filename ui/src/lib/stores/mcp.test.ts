import { describe, expect, it } from 'vitest';
import type { McpConfig } from '$lib/api/generated/McpConfig';
import type { McpStatus } from '$lib/api/generated/McpStatus';
import { FakeTransport } from '$lib/testing/fakeTransport';
import { McpStore, clientConfig } from './mcp.svelte';

const TOOLS = [
  { name: 'get_tree', group: 'read' as const },
  { name: 'get_request', group: 'read' as const },
  { name: 'delete_item', group: 'edit' as const },
  { name: 'send_request', group: 'run' as const },
];

function statusFor(config: McpConfig, extra: Partial<McpStatus> = {}): McpStatus {
  const def = (g: string) => g === 'read' || g === 'run';
  return {
    available: true,
    config,
    tools: TOOLS.map((t) => ({ ...t, enabled: config.tools[t.name] ?? def(t.group) })),
    hasToken: false,
    listen: { endpoint: null, error: null },
    ...extra,
  };
}

function setup() {
  const t = new FakeTransport();
  let config: McpConfig = { enabled: false, port: 7655, tools: {} };
  let hasToken = false;
  t.handle('mcp.config', () => statusFor(config, { hasToken }));
  t.handle('mcp.log', () => [
    { at: 1, tool: 'get_tree', arguments: '{}', ok: true, durationMs: 2 },
  ]);
  t.handle('mcp.setConfig', (input) => {
    config = input;
    const token = input.enabled && !hasToken ? 'tok-1' : undefined;
    if (token) hasToken = true;
    return statusFor(config, { hasToken, ...(token ? { token } : {}) });
  });
  t.handle('mcp.regenerateToken', () => ({ token: 'tok-2' }));
  const store = new McpStore(t);
  return { t, store, config: () => config };
}

describe('McpStore', () => {
  it('loads the status and the call log', async () => {
    const { store } = setup();
    await store.load();
    expect(store.available).toBe(true);
    expect(store.enabled).toBe(false);
    expect(store.log).toHaveLength(1);
    expect(store.groups.map((g) => g.group)).toEqual(['read', 'edit', 'run']);
    expect(store.groups[0]).toMatchObject({ enabled: 2, total: 2 });
    expect(store.groups[1]).toMatchObject({ enabled: 0, total: 1 });
  });

  it('shows the token once, right after the first enable', async () => {
    const { store, config } = setup();
    await store.load();
    await store.setEnabled(true);
    expect(config().enabled).toBe(true);
    expect(store.token).toBe('tok-1');
    expect(store.status?.hasToken).toBe(true);
    store.hideToken();
    expect(store.token).toBeNull();
    await store.regenerate();
    expect(store.token).toBe('tok-2');
  });

  it('saves single tools, whole groups and presets', async () => {
    const { store, config } = setup();
    await store.load();
    await store.setTool('delete_item', true);
    expect(config().tools).toEqual({ delete_item: true });
    await store.setGroup('read', false);
    expect(config().tools).toEqual({ delete_item: true, get_tree: false, get_request: false });
    await store.preset('readOnly');
    expect(store.status!.tools.filter((t) => t.enabled).map((t) => t.name)).toEqual([
      'get_tree',
      'get_request',
    ]);
    await store.preset('all');
    expect(store.status!.tools.every((t) => t.enabled)).toBe(true);
  });

  it('rejects ports outside 1024–65535 without saving', async () => {
    const { store, t } = setup();
    await store.load();
    expect(await store.setPort(80)).toBe(false);
    expect(t.calls.some((c) => c.name === 'mcp.setConfig')).toBe(false);
    expect(await store.setPort(8800)).toBe(true);
  });

  it('follows calls, the listener and changes from other windows', async () => {
    const { store, t } = setup();
    await store.load();
    t.emit({
      topic: 'mcp.call',
      payload: {
        at: 5,
        tool: 'send_request',
        arguments: '{}',
        ok: false,
        error: 'x',
        durationMs: 9,
      },
    });
    expect(store.log[0]!.tool).toBe('send_request');
    t.emit({
      topic: 'mcp.listenChanged',
      payload: { endpoint: 'http://127.0.0.1:7655/mcp', error: null },
    });
    expect(store.status!.listen.endpoint).toBe('http://127.0.0.1:7655/mcp');
    expect(store.endpointUrl('http://server')).toBe('http://127.0.0.1:7655/mcp');
    t.emit({ topic: 'mcp.listenChanged', payload: { endpoint: '/mcp', error: null } });
    expect(store.endpointUrl('https://reqly.example.com')).toBe('https://reqly.example.com/mcp');
  });

  it('is active when enabled and served', async () => {
    const { store, t } = setup();
    await store.load();
    expect(store.active).toBe(false);
    await store.setEnabled(true);
    t.emit({ topic: 'mcp.listenChanged', payload: { endpoint: '/mcp', error: null } });
    expect(store.active).toBe(true);
  });
});

describe('clientConfig', () => {
  const url = 'http://127.0.0.1:7655/mcp';
  it('builds the Claude Code command', () => {
    expect(clientConfig('claude', url, 'abc')).toBe(
      `claude mcp add --transport http reqly ${url} --header "Authorization: Bearer abc"`,
    );
  });
  it('builds Cursor and VS Code JSON', () => {
    expect(JSON.parse(clientConfig('cursor', url, 'abc'))).toEqual({
      mcpServers: { reqly: { url, headers: { Authorization: 'Bearer abc' } } },
    });
    expect(JSON.parse(clientConfig('vscode', url, 'abc'))).toEqual({
      servers: { reqly: { type: 'http', url, headers: { Authorization: 'Bearer abc' } } },
    });
  });
});
