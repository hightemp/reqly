<script lang="ts">
  import type { Auth } from '$lib/api/generated/Auth';
  import { t } from '$lib/i18n';
  import Input from '$lib/ui/Input.svelte';
  import Select from '$lib/ui/overlay/Select.svelte';

  interface Props {
    auth: Auth;
    allowInherit?: boolean;
    onchange: (auth: Auth) => void;
  }

  let { auth, allowInherit = true, onchange }: Props = $props();
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
    <label class="row">
      <span class="label">{t('auth.username')}</span>
      <Input
        mono
        value={auth.username}
        aria-label={t('auth.username')}
        oninput={(e) => onchange({ ...auth, username: e.currentTarget.value })}
      />
    </label>
    <label class="row">
      <span class="label">{t('auth.password')}</span>
      <Input
        mono
        type={reveal ? 'text' : 'password'}
        value={auth.password}
        aria-label={t('auth.password')}
        oninput={(e) => onchange({ ...auth, password: e.currentTarget.value })}
      />
    </label>
    <button type="button" class="reveal" onclick={() => (reveal = !reveal)}
      >{reveal ? t('auth.hide') : t('auth.show')}</button
    >
  {:else if auth.type === 'bearer'}
    <label class="row">
      <span class="label">{t('auth.token')}</span>
      <Input
        mono
        value={auth.token}
        aria-label={t('auth.token')}
        oninput={(e) => onchange({ ...auth, token: e.currentTarget.value })}
      />
    </label>
    <label class="row">
      <span class="label">{t('auth.prefix')}</span>
      <Input
        mono
        value={auth.prefix}
        placeholder={BEARER}
        aria-label={t('auth.prefix')}
        oninput={(e) => onchange({ ...auth, prefix: e.currentTarget.value })}
      />
    </label>
  {:else if auth.type === 'apiKey'}
    <label class="row">
      <span class="label">{t('auth.key')}</span>
      <Input
        mono
        value={auth.key}
        placeholder={API_KEY_HEADER}
        aria-label={t('auth.key')}
        oninput={(e) => onchange({ ...auth, key: e.currentTarget.value })}
      />
    </label>
    <label class="row">
      <span class="label">{t('auth.value')}</span>
      <Input
        mono
        value={auth.value}
        aria-label={t('auth.value')}
        oninput={(e) => onchange({ ...auth, value: e.currentTarget.value })}
      />
    </label>
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
