import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import type { AiChat } from '$lib/api/generated/AiChat';
import type { AiChatMessage } from '$lib/api/generated/AiChatMessage';
import type { AiStatus } from '$lib/api/generated/AiStatus';
import { createServices } from '$lib/services.svelte';
import { Viewport } from '$lib/stores/viewport.svelte';
import { FakeTransport } from '$lib/testing/fakeTransport';
import AiPanel from './AiPanel.svelte';

const media = { matches: false, addEventListener() {}, removeEventListener() {} };

function status(withProvider: boolean): AiStatus {
  return {
    config: {
      enabled: true,
      providers: withProvider
        ? [{ id: 'p1', name: 'Local', preset: 'ollama', baseUrl: 'http://x/v1', headers: [] }]
        : [],
      defaultModel: withProvider ? { providerId: 'p1', model: 'llama3.2' } : null,
      tools: {},
      autoApprove: {},
    },
    providers: withProvider ? [{ id: 'p1', hasKey: false }] : [],
    tools: [],
    presets: [],
    canEdit: true,
  };
}

function msg(
  id: string,
  seq: number,
  role: AiChatMessage['role'],
  extra: Partial<AiChatMessage> = {},
): AiChatMessage {
  return {
    id,
    chatId: 'c1',
    seq,
    role,
    content: '',
    reasoning: '',
    toolCalls: [],
    attachments: [],
    status: 'done',
    createdAt: 0,
    ...extra,
  };
}

async function setup(withProvider = true) {
  const t = new FakeTransport();
  t.withSettings();
  t.withEmptyWorkspace();
  t.handle('ai.config', () => status(withProvider));
  t.handle('ai.models', () => ['llama3.2', 'qwen3']);
  const chat: AiChat = {
    id: 'c1',
    workspaceId: 'w1',
    title: 'Ideas',
    providerId: 'p1',
    model: 'llama3.2',
    autoApprove: false,
    createdAt: 0,
    updatedAt: 0,
  };
  let chats: AiChat[] = [];
  t.handle('ai.chats', () => chats);
  t.handle('ai.createChat', () => {
    chats = [chat];
    return chat;
  });
  t.handle('ai.chat', () => ({ chat, messages: [], running: false, waiting: [] }));
  let finish: () => void = () => {};
  t.handle(
    'ai.send',
    () => new Promise((resolve) => (finish = () => resolve({ status: 'done' as const }))),
  );
  t.handle('ai.confirm', () => ({}));
  t.handle('ai.cancel', () => ({}));
  t.handle('ai.forkChat', () => ({ ...chat, id: 'c2', title: 'Branch: Ideas' }));
  const services = await createServices(t, {
    platform: 'linux',
    media,
    root: document.createElement('div'),
    viewport: new Viewport({ compact: media }),
  });
  const onsettings = vi.fn();
  render(AiPanel, { services, onsettings });
  const emit = (topic: string, payload: unknown) => t.emit({ topic, payload });
  return { t, onsettings, emit, finish: () => finish() };
}

describe('AI panel', () => {
  it('points to the settings until a provider exists', async () => {
    const { onsettings } = await setup(false);
    await fireEvent.click(await screen.findByRole('button', { name: 'Open settings' }));
    expect(onsettings).toHaveBeenCalled();
    expect(screen.queryByRole('textbox')).toBeNull();
  });

  it('sends with Enter, streams the answer and asks before a tool runs', async () => {
    const { t, emit, finish } = await setup();
    const input = await screen.findByRole('textbox', { name: /Ask about your requests/ });
    await fireEvent.input(input, { target: { value: 'Make a request' } });
    await fireEvent.keyDown(input, { key: 'Enter' });
    await waitFor(() => expect(t.calls.some((c) => c.name === 'ai.send')).toBe(true));
    expect(t.calls.find((c) => c.name === 'ai.send')!.input).toEqual({
      chatId: 'c1',
      text: 'Make a request',
      attachments: [],
    });
    expect((input as HTMLTextAreaElement).value).toBe('');
    await screen.findByRole('button', { name: 'Stop' });

    emit('ai.message', {
      chatId: 'c1',
      message: msg('m1', 1, 'user', { content: 'Make a request' }),
    });
    emit('ai.message', { chatId: 'c1', message: msg('m2', 2, 'assistant', { status: 'pending' }) });
    emit('ai.delta', {
      chatId: 'c1',
      messageId: 'm2',
      kind: 'text',
      text: 'Here:\n```json\n{"a": 1}\n```',
    });
    const code = await screen.findByText('"a"');
    expect(code.className).toBe('hl-property');
    // The text can be selected and copied.
    expect(screen.getByText('Make a request').classList).toContain('selectable');
    expect(code.closest('.md')!.classList).toContain('selectable');

    emit('ai.message', {
      chatId: 'c1',
      message: msg('m2', 2, 'assistant', {
        content: 'Here:',
        toolCalls: [{ id: 'call_1', name: 'create_request', arguments: '{"name":"List"}' }],
      }),
    });
    emit('ai.message', {
      chatId: 'c1',
      message: msg('m3', 3, 'tool', {
        toolCallId: 'call_1',
        toolName: 'create_request',
        status: 'pending',
      }),
    });
    emit('ai.confirm', {
      chatId: 'c1',
      messageId: 'm3',
      callId: 'call_1',
      toolName: 'create_request',
      arguments: { name: 'List' },
    });
    await screen.findByText('Allow “create_request”?');
    await fireEvent.click(screen.getByRole('button', { name: 'Allow' }));
    await waitFor(() =>
      expect(t.calls.find((c) => c.name === 'ai.confirm')!.input).toEqual({
        chatId: 'c1',
        callId: 'call_1',
        decision: 'allow',
      }),
    );

    emit('ai.message', {
      chatId: 'c1',
      message: msg('m3', 3, 'tool', {
        toolCallId: 'call_1',
        toolName: 'create_request',
        content: '{"id":"r9"}',
      }),
    });
    emit('ai.done', { chatId: 'c1', status: 'done' });
    finish();
    await screen.findByRole('button', { name: 'Send' });
    expect(screen.getByRole('group', { name: 'create_request' }).textContent).toContain('Done');
  });

  it('stops a running answer', async () => {
    const { t } = await setup();
    const input = await screen.findByRole('textbox', { name: /Ask about your requests/ });
    await fireEvent.input(input, { target: { value: 'Long story' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Send' }));
    await fireEvent.click(await screen.findByRole('button', { name: 'Stop' }));
    await waitFor(() => expect(t.calls.some((c) => c.name === 'ai.cancel')).toBe(true));
  });

  it('shows provider errors in the chat', async () => {
    const { emit } = await setup();
    const input = await screen.findByRole('textbox', { name: /Ask about your requests/ });
    await fireEvent.input(input, { target: { value: 'Hi' } });
    await fireEvent.keyDown(input, { key: 'Enter' });
    await screen.findByRole('button', { name: 'Stop' });
    emit('ai.message', {
      chatId: 'c1',
      message: msg('m2', 2, 'assistant', {
        status: 'error',
        error: {
          code: 'ai.auth',
          message: 'bad key',
          params: { message: 'Incorrect API key', status: 401 },
        },
      }),
    });
    expect((await screen.findByRole('alert')).textContent).toBe(
      'The provider refused the key: Incorrect API key',
    );
  });

  it('copies a message and branches off from an answer', async () => {
    const { t, emit, finish } = await setup();
    const writeText = vi.fn(async () => {});
    Object.defineProperty(navigator, 'clipboard', { value: { writeText }, configurable: true });
    const input = await screen.findByRole('textbox', { name: /Ask about your requests/ });
    await fireEvent.input(input, { target: { value: 'Name it' } });
    await fireEvent.keyDown(input, { key: 'Enter' });
    await screen.findByRole('button', { name: 'Stop' });
    emit('ai.message', { chatId: 'c1', message: msg('m1', 1, 'user', { content: 'Name it' }) });
    emit('ai.message', {
      chatId: 'c1',
      message: msg('m2', 2, 'assistant', { status: 'pending' }),
    });
    emit('ai.delta', { chatId: 'c1', messageId: 'm2', kind: 'text', text: 'Call it **Atlas**' });
    await screen.findByText('Atlas');
    // Not while the answer is still coming.
    expect(screen.queryByRole('button', { name: 'Branch off from here' })).toBeNull();
    emit('ai.message', {
      chatId: 'c1',
      message: msg('m2', 2, 'assistant', { content: 'Call it **Atlas**' }),
    });
    emit('ai.done', { chatId: 'c1', status: 'done' });
    finish();

    await waitFor(() =>
      expect(screen.getAllByRole('button', { name: 'Copy message' })).toHaveLength(2),
    );
    const copies = screen.getAllByRole('button', { name: 'Copy message' });
    await fireEvent.click(copies[1]!);
    expect(writeText).toHaveBeenCalledWith('Call it **Atlas**');

    // Only answers branch off: the new chat keeps everything up to them.
    const branch = screen.getAllByRole('button', { name: 'Branch off from here' });
    expect(branch).toHaveLength(1);
    await fireEvent.click(branch[0]!);
    await waitFor(() =>
      expect(t.calls.find((c) => c.name === 'ai.forkChat')!.input).toEqual({
        chatId: 'c1',
        messageId: 'm2',
        title: 'Branch: Ideas',
      }),
    );
  });
});
