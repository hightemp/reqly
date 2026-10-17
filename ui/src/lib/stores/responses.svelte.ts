// Per-request execution state: in-flight flag and progress, the shown
// response and the history list.

import type { PromptInfo } from '$lib/api/generated/PromptInfo';
import type { ResponseRecord } from '$lib/api/generated/ResponseRecord';
import type { ResponseSummary } from '$lib/api/generated/ResponseSummary';
import { ApiError, type Transport } from '$lib/api/transport';
import { errorMessage } from '$lib/errors';
import { toasts } from '$lib/ui/overlay/toasts.svelte';

export class RequestRun {
  sending = $state(false);
  startedAt = $state<number | null>(null);
  progress = $state<{ downloaded: number; total: number | null } | null>(null);
  current = $state<ResponseRecord | null>(null);
  history = $state<ResponseSummary[]>([]);
  loaded = false;
}

export class ResponseStore {
  // eslint-disable-next-line svelte/prefer-svelte-reactivity -- runs are created lazily during rendering; each run is reactive itself
  private runs = new Map<string, RequestRun>();
  private unsubscribe: () => void;
  /** Asks for `{% prompt %}` values; `null` cancels the send. Set by the shell. */
  askPrompts: (prompts: PromptInfo[]) => Promise<Record<string, string> | null> = () =>
    Promise.resolve(null);

  constructor(
    private readonly transport: Transport,
    /** Saves pending edits before sending. */
    private readonly flushDraft: (requestId: string) => Promise<void>,
  ) {
    this.unsubscribe = transport.subscribe((e) => {
      const p = e.payload as {
        requestId?: string;
        downloaded?: number;
        total?: number | null;
        responseId?: string | null;
      };
      if (!p?.requestId) return;
      const run = this.runs.get(p.requestId);
      if (!run) return;
      if (e.topic === 'request.progress') {
        run.progress = { downloaded: p.downloaded ?? 0, total: p.total ?? null };
      } else if (e.topic === 'request.started') {
        run.sending = true;
      } else if (
        e.topic === 'request.finished' &&
        p.responseId &&
        run.current?.id !== p.responseId
      ) {
        void this.refresh(p.requestId, p.responseId);
      }
    });
  }

  run(requestId: string): RequestRun {
    let run = this.runs.get(requestId);
    if (!run) {
      run = new RequestRun();
      this.runs.set(requestId, run);
    }
    return run;
  }

  async load(requestId: string): Promise<RequestRun> {
    const run = this.run(requestId);
    if (!run.loaded) {
      run.loaded = true;
      await this.refresh(requestId);
    }
    return run;
  }

  /** Reloads history and shows `responseId` (or the newest). */
  private async refresh(requestId: string, responseId?: string) {
    const run = this.run(requestId);
    run.history = await this.transport.call('response.list', { requestId });
    const id = responseId ?? run.history[0]?.id;
    run.current = id ? await this.transport.call('response.get', { id }) : null;
    if (responseId) run.sending = false;
  }

  async send(requestId: string) {
    const run = this.run(requestId);
    if (run.sending) return;
    run.sending = true;
    run.startedAt = Date.now();
    run.progress = null;
    try {
      await this.flushDraft(requestId);
      const prompts = await this.transport
        .call('template.prompts', { requestId })
        .catch(() => [] as PromptInfo[]);
      let promptValues: Record<string, string> | undefined;
      if (prompts.length > 0) {
        const values = await this.askPrompts(prompts);
        if (!values) return;
        promptValues = values;
      }
      const record = await this.transport.call('request.send', { requestId, promptValues });
      run.current = record;
      run.history = await this.transport.call('response.list', { requestId });
    } catch (e) {
      if (ApiError.from(e).code !== 'http.cancelled') toasts.error(errorMessage(e));
    } finally {
      run.sending = false;
      run.progress = null;
      run.startedAt = null;
    }
  }

  async cancel(requestId: string) {
    await this.transport.call('request.cancel', { requestId });
  }

  async select(requestId: string, responseId: string) {
    this.run(requestId).current = await this.transport.call('response.get', { id: responseId });
  }

  async deleteResponse(requestId: string, responseId: string) {
    await this.transport.call('response.delete', { ids: [responseId] });
    const run = this.run(requestId);
    run.history = run.history.filter((h) => h.id !== responseId);
    if (run.current?.id === responseId) {
      const next = run.history[0];
      run.current = next ? await this.transport.call('response.get', { id: next.id }) : null;
    }
  }

  async clear(requestId: string) {
    await this.transport.call('history.clear', { ids: [requestId] });
    const run = this.run(requestId);
    run.history = [];
    run.current = null;
  }

  destroy() {
    this.unsubscribe();
  }
}
