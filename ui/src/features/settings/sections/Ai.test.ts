import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';
import type { AiConfig } from '$lib/api/generated/AiConfig';
import type { AiStatus } from '$lib/api/generated/AiStatus';
import { AiStore } from '$lib/stores/ai.svelte';
import { FakeTransport } from '$lib/testing/fakeTransport';
import { menus } from '$lib/ui/overlay/menus.svelte';
import Ai from './Ai.svelte';

const TOOLS = [
  { name: 'get_tree', group: 'read' as const, ro: true },
  { name: 'create_request', group: 'edit' as const, ro: false },
];

function setup(canEdit = true) {
  const t = new FakeTransport();
  let config: AiConfig = {
    enabled: false,
    providers: [
      {
        id: 'p1',
        name: 'Local',
        preset: 'ollama',
        baseUrl: 'http://localhost:11434/v1',
        headers: [],
      },
    ],
    defaultModel: null,
    tools: {},
    autoApprove: {},
  };
  const keys = new Map<string, string>();
  const status = (): AiStatus => ({
    config,
    providers: config.providers.map((p) => ({ id: p.id, hasKey: keys.has(p.id) })),
    tools: TOOLS.map((x) => ({
      name: x.name,
      group: x.group,
      enabled: config.tools[x.name] ?? true,
      needsConfirmation: !x.ro && !config.autoApprove[x.group],
    })),
    presets: [
      { preset: 'openai', name: 'OpenAI', baseUrl: 'https://api.openai.com/v1', keyRequired: true },
      {
        preset: 'ollama',
        name: 'Ollama',
        baseUrl: 'http://localhost:11434/v1',
        keyRequired: false,
      },
    ],
    canEdit,
  });
  t.handle('ai.config', status);
  t.handle('ai.setConfig', (input) => {
    config = input;
    return status();
  });
  t.handle('ai.setKey', ({ providerId, key }) => {
    if (key) keys.set(providerId, key);
    else keys.delete(providerId);
    return {};
  });
  t.handle('ai.models', () => ['llama3.2', 'qwen3']);
  const ai = new AiStore(t);
  return { t, ai, config: () => config };
}

describe('AI settings', () => {
  it('turns the chat on and adds a provider from a preset', async () => {
    const { ai, config } = setup();
    await ai.load();
    render(Ai, { ai });
    await fireEvent.click(screen.getByRole('switch', { name: 'AI chat' }));
    await waitFor(() => expect(config().enabled).toBe(true));

    await fireEvent.click(screen.getByRole('button', { name: 'Add provider' }));
    const entry = menus.current!.items.find((i) => 'id' in i && i.id === 'openai');
    (entry as { onSelect: () => void }).onSelect();
    await waitFor(() => expect(config().providers).toHaveLength(2));
    expect(config().providers[1]).toMatchObject({ name: 'OpenAI', preset: 'openai' });
    await screen.findByRole('article', { name: 'OpenAI' });
  });

  it('saves a key without ever showing it', async () => {
    const { ai, t } = setup();
    await ai.load();
    render(Ai, { ai });
    const card = screen.getByRole('article', { name: 'Local' });
    expect(card.textContent).toContain('Not needed for this provider');
    const input = screen.getByLabelText('API key') as HTMLInputElement;
    expect(input.type).toBe('password');
    await fireEvent.input(input, { target: { value: 'sk-secret' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Save key' }));
    await screen.findByText('Set');
    expect(input.value).toBe('');
    expect(document.body.textContent).not.toContain('sk-secret');
    expect(t.calls.find((c) => c.name === 'ai.setKey')!.input).toEqual({
      providerId: 'p1',
      key: 'sk-secret',
    });
    await fireEvent.click(screen.getByRole('button', { name: 'Clear' }));
    await waitFor(() => expect(screen.queryByText('Set')).toBeNull());
  });

  it('checks the connection and offers the models for the default', async () => {
    const { ai, config } = setup();
    await ai.load();
    render(Ai, { ai });
    await fireEvent.click(screen.getByRole('button', { name: 'Check connection' }));
    await waitFor(() => expect(ai.models['p1']).toEqual(['llama3.2', 'qwen3']));
    const model = screen.getByRole('combobox', { name: 'Model' }) as HTMLInputElement;
    await fireEvent.focus(model);
    await fireEvent.pointerDown(await screen.findByRole('option', { name: 'qwen3' }));
    await waitFor(() =>
      expect(config().defaultModel).toEqual({ providerId: 'p1', model: 'qwen3' }),
    );
  });

  it('switches tool groups and lets a group run without asking', async () => {
    const { ai, config } = setup();
    await ai.load();
    render(Ai, { ai });
    // Reading never asks, so it has no switch.
    expect(screen.getAllByRole('checkbox', { name: 'Without asking' })).toHaveLength(1);
    await fireEvent.click(screen.getByRole('checkbox', { name: 'Without asking' }));
    await waitFor(() => expect(config().autoApprove).toEqual({ edit: true }));
    await fireEvent.click(screen.getByRole('checkbox', { name: 'Edit requests' }));
    await waitFor(() => expect(config().tools).toEqual({ create_request: false }));
  });

  it('is read-only for users who may not change it', async () => {
    const { ai } = setup(false);
    await ai.load();
    render(Ai, { ai });
    expect(screen.getByRole('note').textContent).toContain('Only an administrator');
    expect(screen.queryByRole('button', { name: 'Add provider' })).toBeNull();
    expect(screen.queryByRole('article')).toBeNull();
    expect(screen.getByText('http://localhost:11434/v1')).toBeTruthy();
    expect(
      (screen.getByRole('switch', { name: 'AI chat' }) as HTMLButtonElement).closest('fieldset')!
        .disabled,
    ).toBe(true);
  });
});
