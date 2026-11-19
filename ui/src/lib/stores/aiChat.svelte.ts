// The AI chat panel's state (ADR-015): the chats of the active workspace,
// the open chat's messages, and a turn in progress followed through
// `ai.*` events.

import type { AiAttachmentRef } from '$lib/api/generated/AiAttachmentRef';
import type { AiChat } from '$lib/api/generated/AiChat';
import type { AiChatMessage } from '$lib/api/generated/AiChatMessage';
import type { AiDecision } from '$lib/api/generated/AiDecision';
import type { AiSendResult } from '$lib/api/generated/AiSendResult';
import { TRANSPORT_TOPICS, type Transport } from '$lib/api/transport';

/** A tool call waiting for the user. */
export interface WaitingCall {
  callId: string;
  messageId: string;
  toolName: string;
  arguments: unknown;
}

interface Payload {
  chatId?: string;
  workspaceId?: string;
  message?: AiChatMessage;
  messageId?: string;
  kind?: 'text' | 'reasoning' | 'toolCall';
  text?: string;
  callId?: string;
  toolName?: string;
  arguments?: unknown;
}

export class AiChatStore {
  workspaceId = $state<string | null>(null);
  chats = $state<AiChat[]>([]);
  activeId = $state<string | null>(null);
  messages = $state<AiChatMessage[]>([]);
  running = $state(false);
  waiting = $state<WaitingCall[]>([]);
  /** The tool the model started calling, by assistant message id. */
  calling = $state<Record<string, string>>({});
  private unsubscribe: () => void;

  constructor(private readonly transport: Transport) {
    this.unsubscribe = transport.subscribe((e) => this.onEvent(e.topic, e.payload as Payload));
  }

  dispose() {
    this.unsubscribe();
  }

  get active(): AiChat | null {
    return this.chats.find((c) => c.id === this.activeId) ?? null;
  }

  private onEvent(topic: string, p: Payload) {
    if (topic === TRANSPORT_TOPICS.reconnected) {
      void this.refresh();
      return;
    }
    if (!topic.startsWith('ai.')) return;
    if (topic === 'ai.chatsChanged') {
      if (p.workspaceId === this.workspaceId) void this.loadChats();
      return;
    }
    if (!p.chatId || p.chatId !== this.activeId) return;
    switch (topic) {
      case 'ai.message': {
        const m = p.message!;
        const i = this.messages.findIndex((x) => x.id === m.id);
        if (i >= 0) this.messages[i] = m;
        else this.messages = [...this.messages, m].sort((a, b) => a.seq - b.seq);
        if (m.role === 'tool' && m.status !== 'pending') {
          this.waiting = this.waiting.filter((w) => w.callId !== m.toolCallId);
        }
        if (m.role === 'user' || m.status === 'pending') this.running = true;
        break;
      }
      case 'ai.delta': {
        const m = this.messages.find((x) => x.id === p.messageId);
        if (!m) break;
        if (p.kind === 'text') m.content += p.text ?? '';
        else if (p.kind === 'reasoning') m.reasoning += p.text ?? '';
        else if (p.kind === 'toolCall') this.calling[m.id] = p.text ?? '';
        break;
      }
      case 'ai.confirm':
        this.waiting = [
          ...this.waiting.filter((w) => w.callId !== p.callId),
          {
            callId: p.callId!,
            messageId: p.messageId!,
            toolName: p.toolName!,
            arguments: p.arguments,
          },
        ];
        break;
      case 'ai.done':
        this.running = false;
        this.waiting = [];
        this.calling = {};
        break;
    }
  }

  /** Shows the chats of `workspaceId` and opens the most recent one. */
  async setWorkspace(workspaceId: string | null) {
    if (workspaceId === this.workspaceId) return;
    this.workspaceId = workspaceId;
    this.activeId = null;
    this.messages = [];
    this.chats = [];
    if (!workspaceId) return;
    await this.loadChats();
    const first = this.chats[0];
    if (first) await this.open(first.id);
  }

  async loadChats() {
    const ws = this.workspaceId;
    if (!ws) return;
    const list = await this.transport.call('ai.chats', { workspaceId: ws });
    if (ws === this.workspaceId) this.chats = list;
  }

  async open(chatId: string) {
    const view = await this.transport.call('ai.chat', { chatId });
    this.activeId = chatId;
    this.messages = view.messages;
    this.running = view.running;
    this.calling = {};
    // After a reload the arguments come with the stored call.
    this.waiting = view.waiting.map((callId) => {
      const tool = view.messages.find((m) => m.role === 'tool' && m.toolCallId === callId);
      const call = view.messages.flatMap((m) => m.toolCalls).find((c) => c.id === callId);
      let args: unknown = call?.arguments;
      try {
        args = JSON.parse(call?.arguments ?? '');
      } catch {
        // Shown as text.
      }
      return {
        callId,
        messageId: tool?.id ?? '',
        toolName: call?.name ?? tool?.toolName ?? '',
        arguments: args,
      };
    });
    const i = this.chats.findIndex((c) => c.id === chatId);
    if (i >= 0) this.chats[i] = view.chat;
  }

  /** Reloads after the event stream was interrupted. */
  async refresh() {
    await this.loadChats();
    if (this.activeId) await this.open(this.activeId).catch(() => {});
  }

  async newChat(): Promise<AiChat | null> {
    if (!this.workspaceId) return null;
    const chat = await this.transport.call('ai.createChat', { workspaceId: this.workspaceId });
    this.chats = [chat, ...this.chats.filter((c) => c.id !== chat.id)];
    this.activeId = chat.id;
    this.messages = [];
    this.running = false;
    this.waiting = [];
    return chat;
  }

  /**
   * Sends a message (in a new chat when none is open) and resolves when the
   * turn ends.
   */
  async send(text: string, attachments: AiAttachmentRef[] = []): Promise<AiSendResult | null> {
    if (!text.trim()) return null;
    const chat = this.active ?? (await this.newChat());
    if (!chat) return null;
    this.running = true;
    try {
      return await this.transport.call('ai.send', { chatId: chat.id, text, attachments });
    } finally {
      if (this.activeId === chat.id) this.running = false;
    }
  }

  async cancel() {
    if (this.activeId) await this.transport.call('ai.cancel', { chatId: this.activeId });
  }

  async confirm(callId: string, decision: AiDecision) {
    if (!this.activeId) return;
    await this.transport.call('ai.confirm', { chatId: this.activeId, callId, decision });
    this.waiting = this.waiting.filter((w) => w.callId !== callId);
  }

  async rename(chatId: string, title: string) {
    const chat = await this.transport.call('ai.renameChat', { chatId, title });
    this.chats = this.chats.map((c) => (c.id === chatId ? chat : c));
  }

  async remove(chatId: string) {
    await this.transport.call('ai.deleteChat', { chatId });
    this.chats = this.chats.filter((c) => c.id !== chatId);
    if (this.activeId === chatId) {
      this.activeId = null;
      this.messages = [];
      this.running = false;
      this.waiting = [];
      const next = this.chats[0];
      if (next) await this.open(next.id);
    }
  }

  async setModel(providerId: string, model: string) {
    const chat = this.active;
    if (!chat) return;
    const updated = await this.transport.call('ai.setChatModel', {
      chatId: chat.id,
      providerId,
      model,
    });
    this.chats = this.chats.map((c) => (c.id === chat.id ? updated : c));
  }
}
