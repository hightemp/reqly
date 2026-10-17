<!-- Request/folder/collection network overrides; empty = inherit. -->
<script lang="ts">
  import type { RequestSettings } from '$lib/api/generated/RequestSettings';
  import { t } from '$lib/i18n';
  import Input from '$lib/ui/Input.svelte';
  import Select from '$lib/ui/overlay/Select.svelte';

  interface Props {
    settings: RequestSettings;
    onchange: (settings: RequestSettings) => void;
  }

  let { settings, onchange }: Props = $props();

  type Flag = 'followRedirects' | 'verifyTls' | 'encodeUrl' | 'sendCookies' | 'storeCookies';
  type Num = 'timeoutMs' | 'connectTimeoutMs' | 'maxRedirects';

  const flags: { key: Flag; label: () => string }[] = [
    { key: 'followRedirects', label: () => t('requestSettings.followRedirects') },
    { key: 'verifyTls', label: () => t('requestSettings.verifyTls') },
    { key: 'encodeUrl', label: () => t('requestSettings.encodeUrl') },
    { key: 'sendCookies', label: () => t('requestSettings.sendCookies') },
    { key: 'storeCookies', label: () => t('requestSettings.storeCookies') },
  ];
  const numbers: { key: Num; label: () => string; unit: () => string }[] = [
    {
      key: 'timeoutMs',
      label: () => t('requestSettings.timeout'),
      unit: () => t('requestSettings.msZeroNone'),
    },
    {
      key: 'connectTimeoutMs',
      label: () => t('requestSettings.connectTimeout'),
      unit: () => t('requestSettings.ms'),
    },
    { key: 'maxRedirects', label: () => t('requestSettings.maxRedirects'), unit: () => '' },
  ];

  const triState = (v: boolean | null | undefined) =>
    v === true ? 'on' : v === false ? 'off' : 'inherit';
  const options = $derived([
    { value: 'inherit', label: t('requestSettings.inherit') },
    { value: 'on', label: t('requestSettings.on') },
    { value: 'off', label: t('requestSettings.off') },
  ]);

  function setFlag(key: Flag, v: string) {
    onchange({ ...settings, [key]: v === 'inherit' ? null : v === 'on' });
  }

  function setNumber(key: Num, raw: string) {
    const n = raw.trim() === '' ? null : Math.max(0, Math.floor(Number(raw)));
    onchange({ ...settings, [key]: Number.isFinite(n) ? n : null });
  }
</script>

<div class="settings">
  {#each flags as f (f.key)}
    <div class="row">
      <span class="label">{f.label()}</span>
      <Select
        label={f.label()}
        value={triState(settings[f.key])}
        {options}
        onchange={(v) => setFlag(f.key, v)}
      />
    </div>
  {/each}
  {#each numbers as n (n.key)}
    <label class="row">
      <span class="label">{n.label()}</span>
      <span class="number">
        <Input
          type="number"
          min="0"
          value={settings[n.key] == null ? '' : String(settings[n.key])}
          placeholder={t('requestSettings.inherit')}
          aria-label={n.label()}
          onchange={(e) => setNumber(n.key, e.currentTarget.value)}
        />
        <span class="unit">{n.unit()}</span>
      </span>
    </label>
  {/each}
</div>

<style>
  .settings {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    max-width: 640px;
  }
  .row {
    display: grid;
    grid-template-columns: 220px 1fr;
    align-items: center;
    gap: var(--space-3);
  }
  .label {
    color: var(--c-fg-muted);
  }
  .number {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .number :global(.input) {
    width: 140px;
  }
  .unit {
    color: var(--c-fg-subtle);
    font-size: var(--font-size-xs);
  }
</style>
