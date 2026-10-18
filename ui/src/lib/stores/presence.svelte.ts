// Who else is in the workspace and which request they have open
// (multi-user server, web). The shell reports where this tab is.

import type { PresencePerson } from '$lib/api/generated/PresencePerson';
import { TRANSPORT_TOPICS, type Transport } from '$lib/api/transport';
import type { AccountStore } from './account.svelte';

export class PresenceStore {
  people = $state<PresencePerson[]>([]);
  private workspaceId: string | null = null;
  private nodeId: string | null = null;
  private unsubscribe: () => void;

  constructor(
    private readonly transport: Transport,
    private readonly account: AccountStore,
  ) {
    this.unsubscribe = transport.subscribe((e) => {
      if (e.topic === TRANSPORT_TOPICS.reconnected) {
        void this.send();
      } else if (e.topic === 'presence.changed') {
        const p = e.payload as { workspaceId: string; people: PresencePerson[] };
        if (p.workspaceId === this.workspaceId) this.people = p.people;
      }
    });
  }

  private get enabled() {
    return this.account.user !== null && !!this.transport.clientId;
  }

  /** Other people in the workspace (one entry per person). */
  get others(): PresencePerson[] {
    const me = this.account.user?.id;
    return this.people.filter(
      (p, i) => p.userId !== me && this.people.findIndex((q) => q.userId === p.userId) === i,
    );
  }

  /** Other people with this request or folder open. */
  on(nodeId: string): PresencePerson[] {
    return this.others.filter((p) =>
      this.people.some((q) => q.userId === p.userId && q.nodeId === nodeId),
    );
  }

  /** Where this tab is now. */
  report(workspaceId: string | null, nodeId: string | null) {
    if (workspaceId !== this.workspaceId) this.people = [];
    this.workspaceId = workspaceId;
    this.nodeId = nodeId;
    void this.send();
  }

  private async send() {
    if (!this.enabled || !this.workspaceId) return;
    try {
      this.people = await this.transport.call('presence.set', {
        clientId: this.transport.clientId!,
        workspaceId: this.workspaceId,
        ...(this.nodeId ? { nodeId: this.nodeId } : {}),
      });
    } catch (e) {
      console.warn('[presence] not sent', e);
    }
  }

  destroy() {
    this.unsubscribe();
  }
}
