// The collection runner: one run at a time, kept while its dialog is closed.

import type { RunItem } from '$lib/api/generated/RunItem';
import type { RunProgress } from '$lib/api/generated/RunProgress';
import type { RunReport } from '$lib/api/generated/RunReport';
import type { RunnerInput } from '$lib/api/generated/RunnerInput';
import type { Transport } from '$lib/api/transport';
import { errorMessage } from '$lib/errors';

export interface RunTarget {
  collectionId: string;
  /** Run this folder instead of the whole collection. */
  folderId: string | null;
  name: string;
}

export type RunOptions = Omit<RunnerInput, 'runId'>;

/** Requests with a failure, each once, in the order they first failed. */
export function failedRequestIds(report: RunReport): string[] {
  const failed = report.items.filter(
    (i) => i.error != null || i.tests.some((t) => !t.passed) || i.assertions.some((a) => !a.passed),
  );
  // eslint-disable-next-line svelte/prefer-svelte-reactivity -- local temporary, not state
  return [...new Set(failed.map((i) => i.requestId))];
}

export class RunnerStore {
  /** The dialog is open for this target. */
  target = $state<RunTarget | null>(null);
  status = $state<'idle' | 'running' | 'done'>('idle');
  runId = $state<string | null>(null);
  items = $state<RunItem[]>([]);
  total = $state(0);
  report = $state<RunReport | null>(null);
  error = $state<string | null>(null);
  private last: RunOptions | null = null;

  constructor(private readonly transport: Transport) {
    transport.subscribe((e) => {
      if (e.topic !== 'runner.progress') return;
      const p = e.payload as RunProgress;
      if (p.runId !== this.runId || this.status !== 'running') return;
      this.items = [...this.items, p.item];
      this.total = p.total;
    });
  }

  open(target: RunTarget) {
    if (
      this.target?.collectionId !== target.collectionId ||
      this.target.folderId !== target.folderId
    ) {
      if (this.status !== 'running') this.reset();
    }
    this.target = target;
  }

  close() {
    this.target = null;
  }

  /** Back to the options (after a run). */
  reset() {
    if (this.status === 'running') return;
    this.status = 'idle';
    this.items = [];
    this.total = 0;
    this.report = null;
    this.error = null;
  }

  async start(options: RunOptions) {
    const runId = crypto.randomUUID();
    this.last = options;
    this.runId = runId;
    this.status = 'running';
    this.items = [];
    this.total = 0;
    this.report = null;
    this.error = null;
    try {
      const report = await this.transport.call('runner.run', { ...options, runId });
      if (this.runId !== runId) return;
      this.report = report;
      this.items = report.items;
    } catch (e) {
      if (this.runId === runId) this.error = errorMessage(e);
    } finally {
      if (this.runId === runId) this.status = 'done';
    }
  }

  async cancel() {
    if (this.runId && this.status === 'running')
      await this.transport.call('runner.cancel', { runId: this.runId });
  }

  async rerunFailed() {
    if (!this.report || !this.last) return;
    const requestIds = failedRequestIds(this.report);
    if (requestIds.length) await this.start({ ...this.last, requestIds });
  }
}
