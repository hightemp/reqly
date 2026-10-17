// Live connections (WebSocket, SSE, GraphQL subscriptions, gRPC) per request:
// status, handshake and the message log, kept in sync with core events.

import type { MessageFormat } from '$lib/api/generated/MessageFormat';
import type { PromptInfo } from '$lib/api/generated/PromptInfo';
import type { StreamInfo } from '$lib/api/generated/StreamInfo';
import type { StreamMessage } from '$lib/api/generated/StreamMessage';
import type { StreamMessageEvent } from '$lib/api/generated/StreamMessageEvent';
import type { Transport } from '$lib/api/transport';
import { errorMessage } from '$lib/errors';
import { toasts } from '$lib/ui/overlay/toasts.svelte';

/** Messages kept in the UI per connection (matches the core's log). */
export const MESSAGE_LIMIT = 5000;

export class StreamRun {
  info = $state<StreamInfo | null>(null);
  messages = $state<StreamMessage[]>([]);
  connecting = $state(false);
  loaded = false;

  get open(): boolean {
    const s = this.info?.status;
    return s === 'open' || s === 'connecting' || s === 'reconnecting';
  }
}

export class StreamStore {
  // eslint-disable-next-line svelte/prefer-svelte-reactivity -- runs are created lazily; each run is reactive itself
  private runs = new Map<string, StreamRun>();
  private unsubscribe: () => void;
  /** Asks for `{% prompt %}` values; `null` cancels. Set by the shell. */
  askPrompts: (prompts: PromptInfo[]) => Promise<Record<string, string> | null> = () =>
    Promise.resolve(null);

  constructor(
    private readonly transport: Transport,
    private readonly flushDraft: (requestId: string) => Promise<void>,
  ) {
    this.unsubscribe = transport.subscribe((e) => {
      if (e.topic === 'stream.status') {
        const info = e.payload as StreamInfo;
        const run = this.runs.get(info.requestId);
        if (!run) return;
        // A new connection replaces the shown one.
        if (run.info && run.info.connectionId !== info.connectionId) run.messages = [];
        run.info = info;
      } else if (e.topic === 'stream.message') {
        const ev = e.payload as StreamMessageEvent;
        const run = this.runs.get(ev.requestId);
        if (!run || (run.info && run.info.connectionId !== ev.connectionId)) return;
        if (run.messages.some((m) => m.seq === ev.message.seq)) return;
        const next = [...run.messages, ev.message];
        run.messages = next.length > MESSAGE_LIMIT ? next.slice(-MESSAGE_LIMIT) : next;
      } else if (e.topic === 'transport.reconnected') {
        for (const [id, run] of this.runs) if (run.loaded) void this.reload(id);
      }
    });
  }

  run(requestId: string): StreamRun {
    let run = this.runs.get(requestId);
    if (!run) {
      run = new StreamRun();
      this.runs.set(requestId, run);
    }
    return run;
  }

  /** Picks up a connection that is already open (after a reload). */
  async load(requestId: string): Promise<StreamRun> {
    const run = this.run(requestId);
    if (!run.loaded) {
      run.loaded = true;
      await this.reload(requestId);
    }
    return run;
  }

  private async reload(requestId: string) {
    const run = this.run(requestId);
    try {
      const list = await this.transport.call('stream.list', {});
      const info = [...list].reverse().find((i) => i.requestId === requestId);
      if (!info) return;
      run.info = info;
      run.messages = await this.transport.call('stream.log', { connectionId: info.connectionId });
    } catch {
      // Nothing open.
    }
  }

  async connect(requestId: string) {
    const run = this.run(requestId);
    run.loaded = true;
    run.connecting = true;
    run.messages = [];
    try {
      await this.flushDraft(requestId);
      const prompts = await this.transport.call('template.prompts', { requestId });
      let values: Record<string, string> | undefined;
      if (prompts.length) {
        const answer = await this.askPrompts(prompts);
        if (!answer) return;
        values = answer;
      }
      const info = await this.transport.call('stream.connect', { requestId, prompts: values });
      // Status events may already be newer than this answer.
      if (run.info?.connectionId !== info.connectionId) run.info = info;
      const log = await this.transport.call('stream.log', { connectionId: info.connectionId });
      // Events may already have delivered some of these.
      const seen: Record<number, true> = {};
      for (const m of run.messages) seen[m.seq] = true;
      run.messages = [...log.filter((m) => !seen[m.seq]), ...run.messages].sort(
        (a, b) => a.seq - b.seq,
      );
    } catch (e) {
      toasts.error(errorMessage(e));
    } finally {
      run.connecting = false;
    }
  }

  async send(requestId: string, format: MessageFormat, body: string): Promise<boolean> {
    const info = this.run(requestId).info;
    if (!info) return false;
    try {
      await this.transport.call('stream.send', { connectionId: info.connectionId, format, body });
      return true;
    } catch (e) {
      toasts.error(errorMessage(e));
      return false;
    }
  }

  /** Ends the client side of a gRPC stream. */
  async end(requestId: string) {
    const info = this.run(requestId).info;
    if (info)
      await this.call(() => this.transport.call('stream.end', { connectionId: info.connectionId }));
  }

  async disconnect(requestId: string) {
    const info = this.run(requestId).info;
    if (info)
      await this.call(() =>
        this.transport.call('stream.disconnect', { connectionId: info.connectionId }),
      );
  }

  async clear(requestId: string) {
    const run = this.run(requestId);
    run.messages = [];
    if (run.info) {
      const id = run.info.connectionId;
      await this.call(() => this.transport.call('stream.clearLog', { connectionId: id }));
    }
  }

  private async call(f: () => Promise<unknown>) {
    try {
      await f();
    } catch (e) {
      toasts.error(errorMessage(e));
    }
  }

  destroy() {
    this.unsubscribe();
  }
}
