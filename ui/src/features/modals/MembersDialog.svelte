<!-- Members of the active workspace and their roles (multi-user server). -->
<script lang="ts">
  import { onMount } from 'svelte';
  import type { Member } from '$lib/api/generated/Member';
  import type { Role } from '$lib/api/generated/Role';
  import { errorMessage } from '$lib/errors';
  import { t } from '$lib/i18n';
  import type { Services } from '$lib/services.svelte';
  import Button from '$lib/ui/Button.svelte';
  import Input from '$lib/ui/Input.svelte';
  import { dialogs } from '$lib/ui/overlay/dialogs.svelte';
  import Modal from '$lib/ui/overlay/Modal.svelte';
  import Select from '$lib/ui/overlay/Select.svelte';
  import { toasts } from '$lib/ui/overlay/toasts.svelte';

  let { services, onclose }: { services: Services; onclose: () => void } = $props();
  const workspace = $derived(services.workspace.activeWorkspace);
  const workspaceId = $derived(services.workspace.activeWorkspaceId);
  const manage = $derived(services.account.canManage(workspaceId));
  const me = $derived(services.account.user?.id ?? null);

  let members = $state<Member[]>([]);
  let username = $state('');
  let role = $state<Role>('editor');
  let busy = $state(false);

  const roles = $derived<{ value: Role; label: string; hint: string }[]>([
    { value: 'viewer', label: t('members.viewer'), hint: t('members.viewerHint') },
    { value: 'editor', label: t('members.editor'), hint: t('members.editorHint') },
    { value: 'owner', label: t('members.owner'), hint: t('members.ownerHint') },
  ]);
  const roleLabel = (r: Role) => roles.find((o) => o.value === r)?.label ?? r;

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
      if (workspaceId) members = await services.transport.call('members.list', { workspaceId });
    });

  const setRole = (name: string, r: Role) =>
    run(async () => {
      if (!workspaceId) return;
      members = await services.transport.call('members.set', {
        workspaceId,
        username: name,
        role: r,
      });
    });

  async function add(e: SubmitEvent) {
    e.preventDefault();
    if (!username.trim()) return;
    await setRole(username.trim(), role);
    username = '';
  }

  async function remove(member: Member) {
    const ok = await dialogs.confirm({
      title: t('members.removeTitle', { name: member.displayName }),
      message: t('members.removeMessage'),
      confirmLabel: t('members.remove'),
      danger: true,
    });
    if (!ok || !workspaceId) return;
    await run(async () => {
      members = await services.transport.call('members.remove', {
        workspaceId,
        userId: member.userId,
      });
    });
  }

  async function leave() {
    const ok = await dialogs.confirm({
      title: t('members.leaveTitle', { name: workspace?.name ?? '' }),
      message: t('members.leaveMessage'),
      confirmLabel: t('members.leave'),
      danger: true,
    });
    if (!ok || !workspaceId) return;
    await run(async () => {
      await services.transport.call('members.leave', { workspaceId });
      onclose();
    });
  }

  onMount(() => void load());
</script>

<Modal
  title={t('members.title', { name: workspace?.name ?? '' })}
  width="min(620px, 94vw)"
  {onclose}
>
  <ul class="list">
    {#each members as member (member.userId)}
      <li>
        <div class="who">
          <span class="name">{member.displayName}</span>
          <span class="user"
            >@{member.username}{member.userId === me ? ` · ${t('members.you')}` : ''}</span
          >
        </div>
        {#if manage}
          <Select
            compact
            label={t('members.role')}
            value={member.role}
            options={roles}
            onchange={(r) => void setRole(member.username, r)}
          />
          <Button size="sm" variant="ghost" disabled={busy} onclick={() => void remove(member)}
            >{t('members.remove')}</Button
          >
        {:else}
          <span class="role">{roleLabel(member.role)}</span>
        {/if}
      </li>
    {/each}
  </ul>

  {#if manage}
    <form class="add" onsubmit={add}>
      <Input
        bind:value={username}
        placeholder={t('members.username')}
        aria-label={t('members.username')}
      />
      <Select compact label={t('members.role')} bind:value={role} options={roles} />
      <Button type="submit" size="sm" variant="primary" disabled={busy || !username.trim()}
        >{t('members.add')}</Button
      >
    </form>
  {:else}
    <div class="footer">
      <Button size="sm" variant="danger" onclick={() => void leave()}>{t('members.leave')}</Button>
    </div>
  {/if}
</Modal>

<style>
  .list {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  li {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-2) 0;
    border-bottom: 1px solid var(--c-border);
  }
  .who {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .user,
  .role {
    color: var(--c-fg-subtle);
    font-size: var(--font-size-sm);
  }
  .add {
    display: flex;
    gap: var(--space-2);
    align-items: center;
    margin-top: var(--space-3);
  }
  .footer {
    display: flex;
    justify-content: flex-end;
    margin-top: var(--space-3);
  }
</style>
