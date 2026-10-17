<!-- Global request defaults (inherited by collections, folders and requests) and proxy. -->
<script lang="ts">
  import { onMount } from 'svelte';
  import type { ProxySettings } from '$lib/api/generated/ProxySettings';
  import type { RequestSettings } from '$lib/api/generated/RequestSettings';
  import { t } from '$lib/i18n';
  import type { Services } from '$lib/services.svelte';
  import Input from '$lib/ui/Input.svelte';
  import Select from '$lib/ui/overlay/Select.svelte';
  import SettingsPanel from '../../request/panels/SettingsPanel.svelte';
  import Field from '../Field.svelte';

  let { services }: { services: Services } = $props();
  const KEY = 'http.defaults';
  let settings = $state<RequestSettings | null>(null);

  onMount(async () => {
    const { value } = await services.transport.call('settings.get', { key: KEY });
    settings = (value ?? {}) as RequestSettings;
  });

  async function save(next: RequestSettings) {
    settings = next;
    await services.transport.call('settings.set', { key: KEY, value: next });
  }

  const proxy = $derived<ProxySettings>(
    settings?.proxy ?? { mode: 'none', url: '', username: '', password: '', noProxy: '' },
  );
  const setProxy = (patch: Partial<ProxySettings>) =>
    settings && void save({ ...settings, proxy: { ...proxy, ...patch } });
</script>

{#if settings}
  <p class="hint">{t('settings.httpHint')}</p>
  <SettingsPanel {settings} onchange={(s) => void save(s)} />

  <h3 class="title">{t('settings.proxy')}</h3>
  <Field label={t('settings.proxyMode')}>
    <Select
      label={t('settings.proxyMode')}
      value={proxy.mode}
      options={[
        { value: 'none', label: t('settings.proxyNone') },
        { value: 'system', label: t('settings.proxySystem') },
        { value: 'manual', label: t('settings.proxyManual') },
      ]}
      onchange={(mode) => setProxy({ mode })}
    />
  </Field>
  {#if proxy.mode === 'manual'}
    <Field label={t('settings.proxyUrl')} hint={t('settings.proxyUrlHint')}>
      <Input
        mono
        value={proxy.url}
        aria-label={t('settings.proxyUrl')}
        onchange={(e) => setProxy({ url: e.currentTarget.value.trim() })}
      />
    </Field>
    <Field label={t('auth.username')}>
      <Input
        value={proxy.username}
        aria-label={t('auth.username')}
        onchange={(e) => setProxy({ username: e.currentTarget.value })}
      />
    </Field>
    <Field label={t('auth.password')}>
      <Input
        type="password"
        value={proxy.password}
        aria-label={t('auth.password')}
        onchange={(e) => setProxy({ password: e.currentTarget.value })}
      />
    </Field>
    <Field label={t('settings.noProxy')} hint={t('settings.noProxyHint')}>
      <Input
        mono
        value={proxy.noProxy}
        aria-label={t('settings.noProxy')}
        onchange={(e) => setProxy({ noProxy: e.currentTarget.value })}
      />
    </Field>
  {/if}
{/if}

<style>
  .hint {
    color: var(--c-fg-subtle);
    margin-bottom: var(--space-3);
  }
  .title {
    font-weight: 600;
    margin: var(--space-4) 0 var(--space-1);
  }
</style>
