<!-- Server administration (multi-user server, administrators only): users,
     all workspaces, sign-up links and the server configuration. -->
<script lang="ts">
  import { onMount } from 'svelte';
  import type { AdminServerOutput } from '$lib/api/generated/AdminServerOutput';
  import type { AdminWorkspace } from '$lib/api/generated/AdminWorkspace';
  import type { Invite } from '$lib/api/generated/Invite';
  import type { User } from '$lib/api/generated/User';
  import { errorMessage } from '$lib/errors';
  import { t } from '$lib/i18n';
  import type { Services } from '$lib/services.svelte';
  import Button from '$lib/ui/Button.svelte';
  import Checkbox from '$lib/ui/Checkbox.svelte';
  import Input from '$lib/ui/Input.svelte';
  import Tabs from '$lib/ui/Tabs.svelte';
  import { dialogs } from '$lib/ui/overlay/dialogs.svelte';
  import { toasts } from '$lib/ui/overlay/toasts.svelte';
  import { inviteLink } from '../../auth/status';
  import Field from '../Field.svelte';

  let { services }: { services: Services } = $props();
  const transport = $derived(services.transport);
  const me = $derived(services.account.user?.id);

  type Tab = 'users' | 'workspaces' | 'server';
  let tab = $state<Tab>('users');
  const tabs = $derived([
    { id: 'users' as const, label: t('admin.users') },
    { id: 'workspaces' as const, label: t('admin.workspaces') },
    { id: 'server' as const, label: t('admin.server') },
  ]);

  let users = $state<User[]>([]);
  let workspaces = $state<AdminWorkspace[]>([]);
  let invites = $state<Invite[]>([]);
  let server = $state<AdminServerOutput | null>(null);
  let link = $state<string | null>(null);
  let busy = $state(false);
  let form = $state({ username: '', password: '', admin: false });

  async function run(action: () => Promise<void>) {
    busy = true;
    try {
      await action();
    } catch (e) {
      toasts.error(errorMessage(e));
    } finally {
      busy = false;
    }
  }

  const load = () =>
    run(async () => {
      [users, workspaces, invites, server] = await Promise.all([
        transport.call('admin.users', {}),
        transport.call('admin.workspaces', {}),
        transport.call('admin.invites', {}),
        transport.call('admin.server', {}),
      ]);
    });

  function replace(user: User) {
    users = users.map((u) => (u.id === user.id ? user : u));
  }

  const createUser = (e: SubmitEvent) => {
    e.preventDefault();
    return run(async () => {
      const user = await transport.call('admin.createUser', {
        username: form.username.trim(),
        password: form.password,
        admin: form.admin,
      });
      users = [...users, user].sort((a, b) => a.username.localeCompare(b.username));
      form = { username: '', password: '', admin: false };
    });
  };

  const update = (user: User, change: { admin?: boolean; disabled?: boolean }) =>
    run(async () =>
      replace(await transport.call('admin.updateUser', { userId: user.id, ...change })),
    );

  async function resetPassword(user: User) {
    const password = await dialogs.prompt({
      title: t('admin.resetPassword', { name: user.username }),
      label: t('account.newPassword'),
      validate: (v) => (v.length >= 8 ? null : t('login.weakPassword')),
    });
    if (!password) return;
    await run(async () => {
      replace(await transport.call('admin.updateUser', { userId: user.id, password }));
      toasts.show({ message: t('admin.passwordReset'), kind: 'success' });
    });
  }

  async function deleteUser(user: User) {
    const ok = await dialogs.confirm({
      title: t('admin.deleteUser', { name: user.username }),
      message: t('admin.deleteUserMessage'),
      confirmLabel: t('tree.delete'),
      danger: true,
    });
    if (!ok) return;
    await run(async () => {
      await transport.call('admin.deleteUser', { userId: user.id });
      users = users.filter((u) => u.id !== user.id);
    });
  }

  const createInvite = () =>
    run(async () => {
      const out = await transport.call('admin.invite', {});
      link = inviteLink(out.token);
      invites = [...invites, out.invite];
    });

  const revokeInvite = (invite: Invite) =>
    run(async () => {
      await transport.call('admin.revokeInvite', { inviteId: invite.id });
      invites = invites.filter((i) => i.id !== invite.id);
    });

  async function deleteWorkspace(ws: AdminWorkspace) {
    const ok = await dialogs.confirm({
      title: t('workspace.deleteTitle', { name: ws.name }),
      message: t('workspace.deleteMessage'),
      confirmLabel: t('tree.delete'),
      danger: true,
    });
    if (!ok) return;
    await run(async () => {
      await transport.call('workspace.delete', { id: ws.id });
      workspaces = workspaces.filter((w) => w.id !== ws.id);
    });
  }

  async function copyLink() {
    if (!link) return;
    await navigator.clipboard.writeText(link);
    toasts.show({ message: t('account.copied'), kind: 'success' });
  }

  const owners = (ws: AdminWorkspace) =>
    ws.members
      .filter((m) => m.role === 'owner')
      .map((m) => m.username)
      .join(', ') || '—';

  // Configuration keys as in reqly.toml.
  const network = $derived.by(() => {
    const info = server?.info;
    if (!info) return '';
    const lines = [
      `network.block_private = ${info.blockPrivate}`,
      `network.block_loopback = ${info.blockLoopback}`,
    ];
    if (info.allowHosts.length) lines.push(`network.allow_hosts = ${info.allowHosts.join(', ')}`);
    if (info.deny.length) lines.push(`network.deny = ${info.deny.join(', ')}`);
    return lines.join('\n');
  });

  onMount(() => void load());
</script>

<Tabs label={t('settings.admin')} idPrefix="admin" {tabs} bind:value={tab} />

{#if tab === 'users'}
  <table>
    <thead>
      <tr>
        <th>{t('login.username')}</th>
        <th>{t('admin.admin')}</th>
        <th>{t('admin.active')}</th>
        <th></th>
      </tr>
    </thead>
    <tbody>
      {#each users as user (user.id)}
        <tr class:disabled={user.disabled}>
          <td>
            <span>{user.displayName}</span>
            <span class="sub">@{user.username}</span>
          </td>
          <td>
            <Checkbox
              checked={user.isAdmin}
              disabled={busy || user.id === me}
              label={t('admin.adminOf', { name: user.username })}
              onchange={(admin) => void update(user, { admin })}
            />
          </td>
          <td>
            <Checkbox
              checked={!user.disabled}
              disabled={busy || user.id === me}
              label={t('admin.activeOf', { name: user.username })}
              onchange={(active) => void update(user, { disabled: !active })}
            />
          </td>
          <td class="end">
            <Button size="sm" variant="ghost" onclick={() => void resetPassword(user)}
              >{t('admin.resetPasswordShort')}</Button
            >
            <Button
              size="sm"
              variant="ghost"
              disabled={user.id === me}
              onclick={() => void deleteUser(user)}>{t('tree.delete')}</Button
            >
          </td>
        </tr>
      {/each}
    </tbody>
  </table>

  <h3>{t('admin.newUser')}</h3>
  <form class="row" onsubmit={createUser}>
    <Input
      bind:value={form.username}
      placeholder={t('login.username')}
      aria-label={t('login.username')}
    />
    <Input
      type="password"
      bind:value={form.password}
      autocomplete="new-password"
      placeholder={t('login.password')}
      aria-label={t('login.password')}
    />
    <Checkbox bind:checked={form.admin} label={t('admin.admin')} showLabel />
    <Button
      type="submit"
      size="sm"
      variant="primary"
      disabled={busy || !form.username.trim() || !form.password}>{t('admin.create')}</Button
    >
  </form>

  <h3>{t('admin.signupLinks')}</h3>
  <p class="hint">{t('admin.signupLinksHint')}</p>
  <Button size="sm" disabled={busy} onclick={() => void createInvite()}
    >{t('members.createInvite')}</Button
  >
  {#if link}
    <div class="link" role="status">
      <code class="selectable">{link}</code>
      <Button size="sm" onclick={() => void copyLink()}>{t('account.copy')}</Button>
    </div>
  {/if}
  {#each invites as invite (invite.id)}
    <div class="row invite">
      <span class="sub"
        >{t('members.inviteExpires', {
          date: new Date(invite.expiresAt).toLocaleDateString(),
        })}</span
      >
      <Button size="sm" variant="ghost" onclick={() => void revokeInvite(invite)}
        >{t('members.revokeInvite')}</Button
      >
    </div>
  {/each}
{:else if tab === 'workspaces'}
  <table>
    <thead>
      <tr>
        <th>{t('admin.workspace')}</th>
        <th>{t('members.owner')}</th>
        <th>{t('admin.members')}</th>
        <th></th>
      </tr>
    </thead>
    <tbody>
      {#each workspaces as ws (ws.id)}
        <tr>
          <td>{ws.name}</td>
          <td>{owners(ws)}</td>
          <td>{ws.members.length}</td>
          <td class="end">
            <Button size="sm" variant="ghost" onclick={() => void deleteWorkspace(ws)}
              >{t('tree.delete')}</Button
            >
          </td>
        </tr>
      {/each}
    </tbody>
  </table>
{:else if server}
  <Field label={t('settings.version')}><span class="mono">{server.version}</span></Field>
  {#if server.dataDir}
    <Field label={t('settings.dataDir')}
      ><span class="mono selectable">{server.dataDir}</span></Field
    >
  {/if}
  <Field label={t('admin.counts')}>
    <span>{t('admin.countsValue', { users: server.users, workspaces: server.workspaces })}</span>
  </Field>
  {#if server.info}
    <Field label={t('admin.authMode')}><span class="mono">{server.info.authMode}</span></Field>
    <Field label={t('admin.registration')} hint={t('admin.configHint')}>
      <span class="mono">{server.info.registration}</span>
    </Field>
    <Field label={t('admin.secureCookie')}>
      <span class="mono">{server.info.secureCookie}</span>
    </Field>
    <Field label={t('admin.network')}>
      <span class="mono">{network}</span>
    </Field>
  {/if}
{/if}

<style>
  table {
    width: 100%;
    border-collapse: collapse;
    margin: var(--space-2) 0;
    font-size: var(--font-size-sm);
  }
  th,
  td {
    text-align: left;
    padding: var(--space-1) var(--space-2);
    border-bottom: 1px solid var(--c-border);
    vertical-align: middle;
  }
  th {
    color: var(--c-fg-subtle);
    font-weight: 500;
  }
  tr.disabled td {
    color: var(--c-fg-subtle);
  }
  .sub {
    display: block;
    color: var(--c-fg-subtle);
    font-size: var(--font-size-xs);
  }
  .end {
    text-align: right;
    white-space: nowrap;
  }
  h3 {
    margin-top: var(--space-4);
    font-size: var(--font-size);
    font-weight: 600;
  }
  .hint {
    color: var(--c-fg-subtle);
    font-size: var(--font-size-sm);
    margin: var(--space-1) 0 var(--space-2);
  }
  .row {
    display: flex;
    gap: var(--space-2);
    align-items: center;
    margin-top: var(--space-2);
  }
  .invite {
    justify-content: space-between;
  }
  .link {
    display: flex;
    gap: var(--space-2);
    align-items: center;
    margin-top: var(--space-2);
    padding: var(--space-2);
    border-radius: var(--radius-sm);
    background: var(--c-bg-sunken);
  }
  .link code,
  .mono {
    font-family: var(--font-mono);
    font-size: var(--font-size-sm);
    word-break: break-all;
  }
  .link code {
    flex: 1;
  }
  .mono {
    white-space: pre-line;
  }
</style>
