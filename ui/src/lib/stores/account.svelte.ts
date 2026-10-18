// The signed-in user of a multi-user server and their workspace roles.
// Without a user (desktop, single-user server) everything is allowed.

import type { Role } from '$lib/api/generated/Role';
import type { User } from '$lib/api/generated/User';
import type { Transport } from '$lib/api/transport';

const RANK: Record<Role, number> = { viewer: 0, editor: 1, owner: 2 };

export class AccountStore {
  user = $state<User | null>(null);
  roles = $state<Partial<Record<string, Role>>>({});
  private unsubscribe: (() => void) | null = null;

  constructor(private readonly transport: Transport) {}

  async load() {
    const me = await this.transport.call('account.me', {});
    this.user = me.user;
    this.roles = me.roles;
    this.unsubscribe ??= this.transport.subscribe((e) => {
      if (e.topic === 'workspaces.changed' && this.user) void this.load();
    });
  }

  /** The caller's role in a workspace; local callers own everything. */
  role(workspaceId: string | null): Role | null {
    if (!this.user || this.user.isAdmin) return 'owner';
    return workspaceId ? (this.roles[workspaceId] ?? null) : null;
  }

  private atLeast(workspaceId: string | null, role: Role): boolean {
    const mine = this.role(workspaceId);
    return mine !== null && RANK[mine] >= RANK[role];
  }

  canEdit(workspaceId: string | null): boolean {
    return this.atLeast(workspaceId, 'editor');
  }

  canManage(workspaceId: string | null): boolean {
    return this.atLeast(workspaceId, 'owner');
  }

  destroy() {
    this.unsubscribe?.();
  }
}
