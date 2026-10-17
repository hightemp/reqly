<script lang="ts">
  import type { Auth } from '$lib/api/generated/Auth';
  import { t } from '$lib/i18n';
  import TemplateInput from '$lib/ui/code/TemplateInput.svelte';
  import Select from '$lib/ui/overlay/Select.svelte';

  interface Props {
    auth: Auth;
    allowInherit?: boolean;
    /** Request or folder whose variables apply. */
    nodeId?: string | null;
    onchange: (auth: Auth) => void;
  }

  let { auth, allowInherit = true, nodeId = null, onchange }: Props = $props();
  let reveal = $state(false);
  // Protocol tokens, not translatable text.
  const BEARER = 'Bearer';
  const API_KEY_HEADER = 'X-API-Key';

  type Kind = Auth['type'];
  const kinds = $derived([
    ...(allowInherit ? [{ value: 'inherit' as Kind, label: t('auth.inherit') }] : []),
    { value: 'none' as Kind, label: t('auth.none') },
    { value: 'basic' as Kind, label: t('auth.basic') },
    { value: 'bearer' as Kind, label: t('auth.bearer') },
    { value: 'apiKey' as Kind, label: t('auth.apiKey') },
  ]);

  function setKind(kind: Kind) {
    if (kind === auth.type) return;
    switch (kind) {
      case 'basic':
        return onchange({ type: 'basic', username: '', password: '' });
      case 'bearer':
        return onchange({ type: 'bearer', token: '', prefix: '' });
      case 'apiKey':
        return onchange({ type: 'apiKey', key: '', value: '', location: 'header' });
      default:
        return onchange({ type: kind } as Auth);
    }
  }
</script>

<div class="auth">
  <label class="row">
    <span class="label">{t('auth.type')}</span>
    <Select label={t('auth.type')} value={auth.type} options={kinds} onchange={setKind} />
  </label>

  {#if auth.type === 'inherit'}
    <p class="hint">{t('auth.inheritHint')}</p>
  {:else if auth.type === 'none'}
    <p class="hint">{t('auth.noneHint')}</p>
  {:else if auth.type === 'basic'}
    <div class="row">
      <span class="label">{t('auth.username')}</span>
      <TemplateInput
        value={auth.username}
        label={t('auth.username')}
        {nodeId}
        onchange={(v) => onchange({ ...auth, username: v })}
      />
    </div>
    <div class="row">
      <span class="label">{t('auth.password')}</span>
      <TemplateInput
        value={auth.password}
        label={t('auth.password')}
        masked={!reveal}
        {nodeId}
        onchange={(v) => onchange({ ...auth, password: v })}
      />
    </div>
    <button type="button" class="reveal" onclick={() => (reveal = !reveal)}
      >{reveal ? t('auth.hide') : t('auth.show')}</button
    >
  {:else if auth.type === 'bearer'}
    <div class="row">
      <span class="label">{t('auth.token')}</span>
      <TemplateInput
        value={auth.token}
        label={t('auth.token')}
        {nodeId}
        onchange={(v) => onchange({ ...auth, token: v })}
      />
    </div>
    <div class="row">
      <span class="label">{t('auth.prefix')}</span>
      <TemplateInput
        value={auth.prefix}
        label={t('auth.prefix')}
        placeholder={BEARER}
        {nodeId}
        onchange={(v) => onchange({ ...auth, prefix: v })}
      />
    </div>
  {:else if auth.type === 'apiKey'}
    <div class="row">
      <span class="label">{t('auth.key')}</span>
      <TemplateInput
        value={auth.key}
        label={t('auth.key')}
        placeholder={API_KEY_HEADER}
        {nodeId}
        onchange={(v) => onchange({ ...auth, key: v })}
      />
    </div>
    <div class="row">
      <span class="label">{t('auth.value')}</span>
      <TemplateInput
        value={auth.value}
        label={t('auth.value')}
        {nodeId}
        onchange={(v) => onchange({ ...auth, value: v })}
      />
    </div>
    <label class="row">
      <span class="label">{t('auth.addTo')}</span>
      <Select
        label={t('auth.addTo')}
        value={auth.location}
        options={[
          { value: 'header', label: t('auth.inHeader') },
          { value: 'query', label: t('auth.inQuery') },
        ]}
        onchange={(location) => onchange({ ...auth, location })}
      />
    </label>
  {/if}
</div>

<style>
  .auth {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    max-width: 640px;
  }
  .row {
    display: grid;
    grid-template-columns: 120px 1fr;
    align-items: center;
    gap: var(--space-3);
  }
  .label {
    color: var(--c-fg-muted);
  }
  .hint {
    color: var(--c-fg-subtle);
  }
  .reveal {
    align-self: flex-start;
    margin-left: calc(120px + var(--space-3));
    color: var(--c-fg-muted);
    font-size: var(--font-size-xs);
  }
</style>
