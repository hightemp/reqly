// AI chat settings (ADR-015): providers and their keys, the default model
// and the tools the chat may use.

import type { AiConfig } from '$lib/api/generated/AiConfig';
import type { AiPreset } from '$lib/api/generated/AiPreset';
import type { AiProvider } from '$lib/api/generated/AiProvider';
import type { AiStatus } from '$lib/api/generated/AiStatus';
import type { McpGroup } from '$lib/api/generated/McpGroup';
import type { Transport } from '$lib/api/transport';
import { GROUPS } from './mcp.svelte';

export interface AiToolState {
  name: string;
  enabled: boolean;
  needsConfirmation: boolean;
}

export interface AiGroupState {
  group: McpGroup;
  tools: AiToolState[];
  enabled: number;
  total: number;
  /** Changing tools of the group run without asking. */
  autoApprove: boolean;
}

export interface AiProviderState extends AiProvider {
  hasKey: boolean;
  keyRequired: boolean;
}

/** A provider id that is not taken yet. */
function newId(taken: string[]): string {
  for (let i = 1; ; i++) {
    const id = `p${i}`;
    if (!taken.includes(id)) return id;
  }
}

export class AiStore {
  status = $state<AiStatus | null>(null);
  /** Models a provider listed, by provider id. */
  models = $state<Record<string, string[]>>({});
  private unsubscribe: () => void;

  constructor(private readonly transport: Transport) {
    this.unsubscribe = transport.subscribe((e) => {
      if (e.topic === 'ai.configChanged') void this.load().catch(() => {});
    });
  }

  dispose() {
    this.unsubscribe();
  }

  get enabled(): boolean {
    return this.status?.config.enabled ?? false;
  }

  get canEdit(): boolean {
    return this.status?.canEdit ?? false;
  }

  get presets() {
    return this.status?.presets ?? [];
  }

  get providers(): AiProviderState[] {
    const status = this.status;
    if (!status) return [];
    return status.config.providers.map((p) => ({
      ...p,
      hasKey: status.providers.find((i) => i.id === p.id)?.hasKey ?? false,
      keyRequired: status.presets.find((s) => s.preset === p.preset)?.keyRequired ?? false,
    }));
  }

  get defaultModel() {
    return this.status?.config.defaultModel ?? null;
  }

  get groups(): AiGroupState[] {
    const status = this.status;
    if (!status) return [];
    return GROUPS.map((group) => {
      const tools = status.tools
        .filter((t) => t.group === group)
        .map((t) => ({
          name: t.name,
          enabled: t.enabled,
          needsConfirmation: t.needsConfirmation,
        }));
      return {
        group,
        tools,
        enabled: tools.filter((t) => t.enabled).length,
        total: tools.length,
        autoApprove: status.config.autoApprove[group] ?? false,
      };
    }).filter((g) => g.total > 0);
  }

  async load() {
    this.status = await this.transport.call('ai.config', {});
  }

  private async save(change: (c: AiConfig) => AiConfig) {
    if (!this.status) return;
    this.status = await this.transport.call('ai.setConfig', change(this.status.config));
  }

  setEnabled(enabled: boolean) {
    return this.save((c) => ({ ...c, enabled }));
  }

  /** Adds a provider with the preset's name and URL; returns its id. */
  async addProvider(preset: AiPreset): Promise<string> {
    const info = this.presets.find((p) => p.preset === preset);
    let id = '';
    await this.save((c) => {
      id = newId(c.providers.map((p) => p.id));
      const provider: AiProvider = {
        id,
        name: info?.name ?? preset,
        preset,
        // A custom provider needs a URL before it can be saved.
        baseUrl: info?.baseUrl || 'http://localhost:8080/v1',
        headers: [],
      };
      return { ...c, providers: [...c.providers, provider] };
    });
    return id;
  }

  updateProvider(id: string, patch: Partial<Omit<AiProvider, 'id'>>) {
    return this.save((c) => ({
      ...c,
      providers: c.providers.map((p) => (p.id === id ? { ...p, ...patch } : p)),
    }));
  }

  async removeProvider(id: string) {
    await this.save((c) => ({
      ...c,
      providers: c.providers.filter((p) => p.id !== id),
      defaultModel: c.defaultModel?.providerId === id ? null : c.defaultModel,
    }));
    const { [id]: _, ...rest } = this.models;
    this.models = rest;
  }

  /** Stores (or with `null`, clears) a provider's key. */
  async setKey(providerId: string, key: string | null) {
    await this.transport.call('ai.setKey', { providerId, key });
    await this.load();
  }

  /** Asks the provider for its models (also a connection check). */
  async loadModels(providerId: string): Promise<string[]> {
    const list = await this.transport.call('ai.models', { providerId });
    this.models = { ...this.models, [providerId]: list };
    return list;
  }

  setDefaultModel(providerId: string, model: string) {
    return this.save((c) => ({ ...c, defaultModel: { providerId, model } }));
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

  setAutoApprove(group: McpGroup, on: boolean) {
    return this.save((c) => ({ ...c, autoApprove: { ...c.autoApprove, [group]: on } }));
  }
}
