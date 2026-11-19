import { describe, expect, it } from 'vitest';
import type { AiConfig } from '$lib/api/generated/AiConfig';
import type { AiStatus } from '$lib/api/generated/AiStatus';
import { FakeTransport } from '$lib/testing/fakeTransport';
import { AiStore } from './ai.svelte';

const TOOLS = [
  { name: 'get_tree', group: 'read' as const, ro: true },
  { name: 'create_request', group: 'edit' as const, ro: false },
  { name: 'send_request', group: 'run' as const, ro: false },
];

const PRESETS: AiStatus['presets'] = [
  { preset: 'openai', name: 'OpenAI', baseUrl: 'https://api.openai.com/v1', keyRequired: true },
  { preset: 'ollama', name: 'Ollama', baseUrl: 'http://localhost:11434/v1', keyRequired: false },
  { preset: 'custom', name: 'OpenAI-compatible', baseUrl: '', keyRequired: false },
];

function setup() {
  const t = new FakeTransport();
  let config: AiConfig = {
    enabled: false,
    providers: [],
    defaultModel: null,
    tools: {},
    autoApprove: {},
  };
  const keys = new Set<string>();
  const status = (): AiStatus => ({
    config,
    providers: config.providers.map((p) => ({ id: p.id, hasKey: keys.has(p.id) })),
    tools: TOOLS.map((tool) => ({
      name: tool.name,
      group: tool.group,
      enabled: config.tools[tool.name] ?? true,
      needsConfirmation: !tool.ro && !config.autoApprove[tool.group],
    })),
    presets: PRESETS,
    canEdit: true,
  });
  t.handle('ai.config', status);
  t.handle('ai.setConfig', (input) => {
    config = input;
    return status();
  });
  t.handle('ai.setKey', ({ providerId, key }) => {
    if (key) keys.add(providerId);
    else keys.delete(providerId);
    return {};
  });
  t.handle('ai.models', () => ['m-1', 'm-2']);
  return { t, store: new AiStore(t), config: () => config };
}

describe('AiStore', () => {
  it('adds providers from presets with fresh ids', async () => {
    const { store, config } = setup();
    await store.load();
    expect(store.enabled).toBe(false);
    const a = await store.addProvider('openai');
    const b = await store.addProvider('ollama');
    const c = await store.addProvider('custom');
    expect([a, b, c]).toEqual(['p1', 'p2', 'p3']);
    expect(config().providers[0]).toMatchObject({
      name: 'OpenAI',
      baseUrl: 'https://api.openai.com/v1',
    });
    // A custom provider starts with a placeholder URL it can be saved with.
    expect(config().providers[2]!.baseUrl).toMatch(/^http/);
    expect(store.providers.map((p) => p.keyRequired)).toEqual([true, false, false]);
  });

  it('keys are write-only: the store only learns that one is set', async () => {
    const { store } = setup();
    await store.load();
    const id = await store.addProvider('openai');
    await store.setKey(id, 'sk-1');
    expect(store.providers[0]!.hasKey).toBe(true);
    expect(JSON.stringify(store.status)).not.toContain('sk-1');
    await store.setKey(id, null);
    expect(store.providers[0]!.hasKey).toBe(false);
  });

  it('lists models and picks the default; removing the provider clears it', async () => {
    const { store, config } = setup();
    await store.load();
    const id = await store.addProvider('ollama');
    expect(await store.loadModels(id)).toEqual(['m-1', 'm-2']);
    expect(store.models[id]).toEqual(['m-1', 'm-2']);
    await store.setDefaultModel(id, 'm-2');
    expect(store.defaultModel).toEqual({ providerId: id, model: 'm-2' });
    await store.removeProvider(id);
    expect(config().defaultModel).toBeNull();
    expect(store.models[id]).toBeUndefined();
  });

  it('groups tools and auto-approves a group', async () => {
    const { store } = setup();
    await store.load();
    expect(store.groups.map((g) => [g.group, g.enabled, g.total])).toEqual([
      ['read', 1, 1],
      ['edit', 1, 1],
      ['run', 1, 1],
    ]);
    await store.setGroup('edit', false);
    expect(store.groups[1]!.enabled).toBe(0);
    await store.setAutoApprove('run', true);
    expect(store.groups[2]!.autoApprove).toBe(true);
    expect(store.groups[2]!.tools[0]!.needsConfirmation).toBe(false);
  });

  it('reloads when the settings change elsewhere', async () => {
    const { t, store, config } = setup();
    await store.load();
    config().enabled = true;
    t.emit({ topic: 'ai.configChanged', payload: { enabled: true } });
    await Promise.resolve();
    await Promise.resolve();
    expect(store.enabled).toBe(true);
  });
});
