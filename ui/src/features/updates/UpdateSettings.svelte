<!-- Settings → About: check for updates and update preferences. -->
<script lang="ts">
  import type { UpdateChannel } from '$lib/api/generated/UpdateChannel';
  import { i18n, t } from '$lib/i18n';
  import type { UpdatesStore } from '$lib/stores/updates.svelte';
  import Button from '$lib/ui/Button.svelte';
  import Select from '$lib/ui/overlay/Select.svelte';
  import Switch from '$lib/ui/Switch.svelte';
  import Field from '../settings/Field.svelte';

  let { updates }: { updates: UpdatesStore } = $props();

  const status = $derived.by(() => {
    if (!updates.enabled) return t('updates.disabled');
    switch (updates.status) {
      case 'checking':
        return t('updates.checking');
      case 'upToDate':
        return t('updates.upToDate');
      case 'error':
        return updates.error ?? '';
      case 'available':
      case 'downloading':
        return updates.release ? t('updates.found', { version: updates.release.version }) : '';
      case 'ready':
        return t('updates.ready');
      default:
        return updates.checkedAt
          ? t('updates.lastChecked', { time: i18n.formatDateTime(updates.checkedAt) })
          : '';
    }
  });
  const hasRelease = $derived(
    updates.release !== null &&
      ['available', 'downloading', 'ready', 'error'].includes(updates.status),
  );
</script>

<h3 class="title">{t('updates.heading')}</h3>
<Field label={t('updates.check')} hint={status}>
  <div class="row">
    {#if hasRelease}
      <Button size="sm" onclick={() => (updates.dialogOpen = true)}>{t('updates.details')}</Button>
    {/if}
    <Button
      size="sm"
      loading={updates.status === 'checking'}
      disabled={!updates.enabled || updates.status === 'downloading'}
      onclick={() => void updates.check(true)}>{t('updates.check')}</Button
    >
  </div>
</Field>
{#if updates.enabled}
  <Field label={t('updates.autoCheck')}>
    <Switch
      label={t('updates.autoCheck')}
      checked={updates.settings.autoCheck}
      onchange={(v) => void updates.saveSettings({ autoCheck: v })}
    />
  </Field>
  {#if updates.installable}
    <Field label={t('updates.autoInstall')}>
      <Switch
        label={t('updates.autoInstall')}
        checked={updates.settings.autoInstall}
        onchange={(v) => void updates.saveSettings({ autoInstall: v })}
      />
    </Field>
  {/if}
  <Field label={t('updates.channel')}>
    <Select
      label={t('updates.channel')}
      value={updates.settings.channel}
      options={[
        { value: 'stable', label: t('updates.channels.stable') },
        { value: 'prerelease', label: t('updates.channels.prerelease') },
      ]}
      onchange={async (v: UpdateChannel) => {
        await updates.saveSettings({ channel: v });
        await updates.check(false);
      }}
    />
  </Field>
{/if}

<style>
  .title {
    margin: var(--space-4) 0 var(--space-2);
    font-size: var(--font-size-sm);
  }
  .row {
    display: flex;
    gap: var(--space-2);
    justify-content: flex-end;
  }
</style>
