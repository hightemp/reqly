// The MCP server for agents (ADR-014): its settings, the token shown once
// and the log of agent calls.

import type { McpCall } from '$lib/api/generated/McpCall';
import type { McpConfig } from '$lib/api/generated/McpConfig';
import type { McpGroup } from '$lib/api/generated/McpGroup';
import type { McpListen } from '$lib/api/generated/McpListen';
import type { McpStatus } from '$lib/api/generated/McpStatus';
import type { Transport } from '$lib/api/transport';

const LOG_SIZE = 200;
export const GROUPS: McpGroup[] = ['read', 'edit', 'run', 'environments', 'interop'];
export type McpClient = 'claude' | 'cursor' | 'vscode';

export interface GroupState {
  group: McpGroup;
  tools: { name: string; enabled: boolean }[];
  enabled: number;
  total: number;
}

/** What to paste into an MCP client. */
export function clientConfig(client: McpClient, url: string, token: string): string {
  const headers = { Authorization: `Bearer ${token}` };
  switch (client) {
    case 'claude':
      return `claude mcp add --transport http reqly ${url} --header "Authorization: Bearer ${token}"`;
    case 'cursor':
      return JSON.stringify({ mcpServers: { reqly: { url, headers } } }, null, 2);
    case 'vscode':
      return JSON.stringify({ servers: { reqly: { type: 'http', url, headers } } }, null, 2);
  }
}

export class McpStore {
  status = $state<McpStatus | null>(null);
  log = $state<McpCall[]>([]);
  /** The plain token, until the user hides it. */
  token = $state<string | null>(null);
  private unsubscribe: () => void;

  constructor(private readonly transport: Transport) {
    this.unsubscribe = transport.subscribe((e) => {
      if (e.topic === 'mcp.call') {
        this.log = [e.payload as McpCall, ...this.log].slice(0, LOG_SIZE);
      } else if (e.topic === 'mcp.listenChanged' && this.status) {
        this.status = { ...this.status, listen: e.payload as McpListen };
      } else if (e.topic === 'mcp.configChanged') {
        void this.loadStatus();
      }
    });
  }

  dispose() {
    this.unsubscribe();
  }

  get available(): boolean {
    return this.status?.available ?? false;
  }

  get enabled(): boolean {
    return this.status?.config.enabled ?? false;
  }

  /** Enabled and actually served (the desktop port may be busy). */
  get active(): boolean {
    return this.enabled && !!this.status?.listen.endpoint;
  }

  get groups(): GroupState[] {
    const tools = this.status?.tools ?? [];
    return GROUPS.map((group) => {
      const list = tools
        .filter((t) => t.group === group)
        .map((t) => ({ name: t.name, enabled: t.enabled }));
      return {
        group,
        tools: list,
        enabled: list.filter((t) => t.enabled).length,
        total: list.length,
      };
    }).filter((g) => g.total > 0);
  }

  /** The endpoint as a full URL (the server reports a path). */
  endpointUrl(origin: string): string | null {
    const endpoint = this.status?.listen.endpoint;
    if (!endpoint) return null;
    return endpoint.startsWith('/') ? `${origin}${endpoint}` : endpoint;
  }

  async load() {
    await this.loadStatus();
    this.log = await this.transport.call('mcp.log', {});
  }

  private async loadStatus() {
    this.status = await this.transport.call('mcp.config', {});
  }

  private async save(change: (c: McpConfig) => McpConfig) {
    if (!this.status) return;
    const next = await this.transport.call('mcp.setConfig', change(this.status.config));
    const { token, ...status } = next;
    this.status = status;
    if (token) this.token = token;
  }

  setEnabled(enabled: boolean) {
    return this.save((c) => ({ ...c, enabled }));
  }

  /** Saves a valid port; `false` when out of range. */
  async setPort(port: number): Promise<boolean> {
    if (!Number.isInteger(port) || port < 1024 || port > 65535) return false;
    await this.save((c) => ({ ...c, port }));
    return true;
  }

  setTool(name: string, enabled: boolean) {
    return this.save((c) => ({ ...c, tools: { ...c.tools, [name]: enabled } }));
  }

  setGroup(group: McpGroup, enabled: boolean) {
    const names = (this.status?.tools ?? []).filter((t) => t.group === group).map((t) => t.name);
    return this.save((c) => ({
      ...c,
      tools: { ...c.tools, ...Object.fromEntries(names.map((n) => [n, enabled])) },
    }));
  }

  preset(kind: 'readOnly' | 'all') {
    const tools = Object.fromEntries(
      (this.status?.tools ?? []).map((t) => [t.name, kind === 'all' || t.group === 'read']),
    );
    return this.save((c) => ({ ...c, tools }));
  }

  async regenerate() {
    const { token } = await this.transport.call('mcp.regenerateToken', {});
    this.token = token;
    if (this.status) this.status = { ...this.status, hasToken: true };
  }

  hideToken() {
    this.token = null;
  }
}
