// Desktop ↔ server sync (docs/sync.md): the connection, which workspaces
// sync, their status and the conflicts waiting for the user.

import type { SyncConflict } from '$lib/api/generated/SyncConflict';
import type { SyncConnectInput } from '$lib/api/generated/SyncConnectInput';
import type { SyncLogEntry } from '$lib/api/generated/SyncLogEntry';
import type { SyncResolution } from '$lib/api/generated/SyncResolution';
import type { SyncStatus } from '$lib/api/generated/SyncStatus';
import type { SyncWorkspace } from '$lib/api/generated/SyncWorkspace';
import type { Transport } from '$lib/api/transport';
import { errorMessage } from '$lib/errors';

/** What the status bar shows. */
export type SyncState = 'off' | 'ok' | 'pending' | 'syncing' | 'error' | 'conflicts';

export class SyncStore {
  status = $state<SyncStatus | null>(null);
  workspaces = $state<SyncWorkspace[]>([]);
  /** Why the server's workspaces are missing from `workspaces`. */
  workspacesError = $state<string | null>(null);
  conflicts = $state<SyncConflict[]>([]);
  log = $state<SyncLogEntry[]>([]);
  /** The error of the last action (connect, sync now…). */
  error = $state<string | null>(null);
  busy = $state(false);
  conflictsOpen = $state(false);
  private unsubscribe: () => void;

  constructor(private readonly transport: Transport) {
    this.unsubscribe = transport.subscribe((e) => {
      if (e.topic !== 'sync.status') return;
      const next = e.payload as SyncStatus;
      const conflictsChanged = next.conflicts !== this.status?.conflicts;
      this.status = next;
      if (conflictsChanged) void this.loadConflicts();
    });
  }

  dispose() {
    this.unsubscribe();
  }

  get connected(): boolean {
    return !!this.status?.url;
  }

  get state(): SyncState {
    const s = this.status;
    if (!s?.url) return 'off';
    if (s.conflicts > 0) return 'conflicts';
    if (s.syncing) return 'syncing';
    if (s.lastError) return 'error';
    if (s.pending > 0) return 'pending';
    return 'ok';
  }

  async load() {
    this.status = await this.transport.call('sync.status', {});
    await this.loadConflicts();
  }

  async loadConflicts() {
    this.conflicts = this.connected ? await this.transport.call('sync.conflicts', {}) : [];
  }

  async loadLog() {
    this.log = await this.transport.call('sync.log', {});
  }

  async loadWorkspaces() {
    const out = await this.transport.call('sync.workspaces', {});
    this.workspaces = out.workspaces;
    this.workspacesError = out.error ? errorMessage(out.error) : null;
  }

  /** Runs an action, keeping its error for the UI; false when it failed. */
  private async run(action: () => Promise<SyncStatus>): Promise<boolean> {
    this.busy = true;
    this.error = null;
    try {
      this.status = await action();
      return true;
    } catch (e) {
      this.error = errorMessage(e);
      return false;
    } finally {
      this.busy = false;
    }
  }

  connect(input: SyncConnectInput): Promise<boolean> {
    return this.run(() => this.transport.call('sync.connect', input));
  }

  async disconnect() {
    await this.run(() => this.transport.call('sync.disconnect', {}));
    this.workspaces = [];
    this.conflicts = [];
  }

  async syncNow() {
    await this.run(() => this.transport.call('sync.now', {}));
    await this.loadLog();
  }

  async setSecrets(secrets: boolean) {
    await this.run(() => this.transport.call('sync.settings', { secrets }));
  }

  /** Starts (uploading or downloading) or stops syncing a workspace. */
  async setSynced(workspaceId: string, synced: boolean) {
    const ok = await this.run(() =>
      this.transport.call(synced ? 'sync.follow' : 'sync.unfollow', { workspaceId }),
    );
    if (ok) await this.loadWorkspaces();
  }

  async resolve(conflict: SyncConflict, resolution: SyncResolution) {
    const ok = await this.run(() =>
      this.transport.call('sync.resolve', {
        kind: conflict.local.kind,
        id: conflict.local.id,
        resolution,
      }),
    );
    if (ok) await this.loadConflicts();
    if (this.conflicts.length === 0) this.conflictsOpen = false;
  }
}
