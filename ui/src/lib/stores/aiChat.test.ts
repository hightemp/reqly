import { describe, expect, it } from 'vitest';
import type { AiChat } from '$lib/api/generated/AiChat';
import type { AiChatMessage } from '$lib/api/generated/AiChatMessage';
import { FakeTransport } from '$lib/testing/fakeTransport';
import { AiChatStore } from './aiChat.svelte';

function chat(id: string, title = ''): AiChat {
  return {
    id,
    workspaceId: 'w1',
    title,
    providerId: 'p1',
    model: 'm',
    autoApprove: false,
    createdAt: 0,
    updatedAt: 0,
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

function setup() {
  const t = new FakeTransport();
  let chats = [chat('c2', 'Newer'), chat('c1', 'Older')];
  const stored: Record<string, AiChatMessage[]> = {
    c1: [msg('m1', 1, 'user', { content: 'hi' })],
    c2: [],
  };
  let waiting: string[] = [];
  t.handle('ai.chats', () => chats);
  t.handle('ai.chat', ({ chatId }) => ({
    chat: chats.find((c) => c.id === chatId)!,
    messages: stored[chatId] ?? [],
    running: waiting.length > 0,
    waiting,
  }));
  t.handle('ai.createChat', () => {
    const c = chat('c3');
    chats = [c, ...chats];
    return c;
  });
  t.handle('ai.deleteChat', ({ chatId }) => {
    chats = chats.filter((c) => c.id !== chatId);
    return {};
  });
  let release: () => void = () => {};
  t.handle(
    'ai.send',
    () => new Promise((resolve) => (release = () => resolve({ status: 'done' as const }))),
  );
  t.handle('ai.cancel', () => ({}));
  t.handle('ai.confirm', () => ({}));
  t.handle('ai.forkChat', ({ chatId, messageId, title }) => {
    const c = { ...chat('c9'), title: title ?? '' };
    chats = [c, ...chats];
    stored.c9 = stored[chatId]!.filter(
      (m) => m.seq <= stored[chatId]!.find((x) => x.id === messageId)!.seq,
    );
    return c;
  });
  t.handle('ai.renameChat', ({ chatId, title }) => ({ ...chat(chatId), title }));
  t.handle('ai.setChatModel', ({ chatId, providerId, model }) => ({
    ...chat(chatId),
    providerId,
    model,
  }));
  const store = new AiChatStore(t);
  return {
    t,
    store,
    release: () => release(),
    setWaiting: (w: string[]) => (waiting = w),
    stored,
  };
}

const emit = (t: FakeTransport, topic: string, payload: unknown) => t.emit({ topic, payload });

describe('AiChatStore', () => {
  it('opens the most recent chat of the workspace', async () => {
    const { store } = setup();
    await store.setWorkspace('w1');
    expect(store.chats.map((c) => c.id)).toEqual(['c2', 'c1']);
    expect(store.activeId).toBe('c2');
    await store.open('c1');
    expect(store.messages.map((m) => m.content)).toEqual(['hi']);
  });

  it('follows a turn through events', async () => {
    const { t, store, release } = setup();
    await store.setWorkspace('w1');
    await store.open('c1');
    const sending = store.send('Why 500?');
    expect(store.running).toBe(true);
    emit(t, 'ai.message', { chatId: 'c1', message: msg('m2', 2, 'user', { content: 'Why 500?' }) });
    emit(t, 'ai.message', {
      chatId: 'c1',
      message: msg('m3', 3, 'assistant', { status: 'pending' }),
    });
    emit(t, 'ai.delta', { chatId: 'c1', messageId: 'm3', kind: 'reasoning', text: 'hmm' });
    emit(t, 'ai.delta', { chatId: 'c1', messageId: 'm3', kind: 'text', text: 'Because ' });
    emit(t, 'ai.delta', { chatId: 'c1', messageId: 'm3', kind: 'text', text: 'X.' });
    emit(t, 'ai.delta', { chatId: 'c1', messageId: 'm3', kind: 'toolCall', text: 'get_tree' });
    expect(store.messages.map((m) => m.id)).toEqual(['m1', 'm2', 'm3']);
    expect(store.messages[2]!.content).toBe('Because X.');
    expect(store.messages[2]!.reasoning).toBe('hmm');
    expect(store.calling['m3']).toBe('get_tree');

    // Another chat's events change nothing here.
    emit(t, 'ai.delta', { chatId: 'c2', messageId: 'm3', kind: 'text', text: '!!' });
    expect(store.messages[2]!.content).toBe('Because X.');

    // The final state replaces the streamed one.
    emit(t, 'ai.message', {
      chatId: 'c1',
      message: msg('m3', 3, 'assistant', { content: 'Because X.', status: 'done' }),
    });
    emit(t, 'ai.done', { chatId: 'c1', status: 'done' });
    expect(store.running).toBe(false);
    expect(store.calling).toEqual({});
    release();
    expect(await sending).toEqual({ status: 'done' });
    expect(t.calls.find((c) => c.name === 'ai.send')!.input).toEqual({
      chatId: 'c1',
      text: 'Why 500?',
      attachments: [],
    });
  });

  it('tracks tool calls that wait and answers them', async () => {
    const { t, store } = setup();
    await store.setWorkspace('w1');
    await store.open('c1');
    emit(t, 'ai.confirm', {
      chatId: 'c1',
      messageId: 'm9',
      callId: 'call_1',
      toolName: 'create_request',
      arguments: { name: 'x' },
    });
    expect(store.waiting).toEqual([
      { callId: 'call_1', messageId: 'm9', toolName: 'create_request', arguments: { name: 'x' } },
    ]);
    await store.confirm('call_1', 'allow');
    expect(store.waiting).toEqual([]);
    expect(t.calls.find((c) => c.name === 'ai.confirm')!.input).toEqual({
      chatId: 'c1',
      callId: 'call_1',
      decision: 'allow',
    });
    // A finished tool message also ends the wait.
    emit(t, 'ai.confirm', { chatId: 'c1', messageId: 'm9', callId: 'call_2', toolName: 't' });
    emit(t, 'ai.message', {
      chatId: 'c1',
      message: msg('m9', 9, 'tool', { toolCallId: 'call_2', status: 'denied' }),
    });
    expect(store.waiting).toEqual([]);
  });

  it('restores a waiting call after reopening the chat', async () => {
    const { store, setWaiting, stored } = setup();
    stored['c1'] = [
      msg('m1', 1, 'user', { content: 'go' }),
      msg('m2', 2, 'assistant', {
        toolCalls: [{ id: 'call_7', name: 'delete_item', arguments: '{"id":"n1"}' }],
      }),
      msg('m3', 3, 'tool', { toolCallId: 'call_7', toolName: 'delete_item', status: 'pending' }),
    ];
    setWaiting(['call_7']);
    await store.setWorkspace('w1');
    await store.open('c1');
    expect(store.running).toBe(true);
    expect(store.waiting).toEqual([
      { callId: 'call_7', messageId: 'm3', toolName: 'delete_item', arguments: { id: 'n1' } },
    ]);
  });

  it('a message without an open chat starts one', async () => {
    const { t, store, release } = setup();
    await store.setWorkspace('w1');
    store.activeId = null;
    const sending = store.send('hello');
    await new Promise((r) => setTimeout(r, 0));
    expect(store.activeId).toBe('c3');
    release();
    await sending;
    expect(t.calls.find((c) => c.name === 'ai.send')!.input).toMatchObject({ chatId: 'c3' });
  });

  it('deleting the open chat opens the next one; renames and models update the list', async () => {
    const { store } = setup();
    await store.setWorkspace('w1');
    await store.rename('c2', 'Renamed');
    expect(store.chats[0]!.title).toBe('Renamed');
    await store.setModel('p2', 'big');
    expect(store.active!.model).toBe('big');
    await store.remove('c2');
    expect(store.chats.map((c) => c.id)).toEqual(['c1']);
    expect(store.activeId).toBe('c1');
  });

  it('reloads the chat list when it changes', async () => {
    const { t, store } = setup();
    await store.setWorkspace('w1');
    emit(t, 'ai.chatsChanged', { workspaceId: 'w1' });
    await Promise.resolve();
    expect(t.calls.filter((c) => c.name === 'ai.chats')).toHaveLength(2);
    emit(t, 'ai.chatsChanged', { workspaceId: 'other' });
    expect(t.calls.filter((c) => c.name === 'ai.chats')).toHaveLength(2);
  });

  it('branches off into a new chat and opens it', async () => {
    const { store, t, stored } = setup();
    stored.c1 = [
      msg('m1', 1, 'user', { content: 'hi' }),
      msg('m2', 2, 'assistant', { content: 'hello' }),
      msg('m3', 3, 'user', { content: 'more' }),
    ];
    await store.setWorkspace('w1');
    await store.open('c1');
    await store.fork('m2', 'Branch: Older');
    expect(t.calls.find((c) => c.name === 'ai.forkChat')!.input).toEqual({
      chatId: 'c1',
      messageId: 'm2',
      title: 'Branch: Older',
    });
    expect(store.activeId).toBe('c9');
    expect(store.chats[0]!.id).toBe('c9');
    expect(store.messages.map((m) => m.content)).toEqual(['hi', 'hello']);
  });
});
