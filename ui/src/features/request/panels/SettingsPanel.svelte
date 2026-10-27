<!-- Request/folder/collection network overrides; empty = inherit. -->
<script lang="ts">
  import type { HttpVersion } from '$lib/api/generated/HttpVersion';
  import type { RequestSettings } from '$lib/api/generated/RequestSettings';
  import type { TlsVersion } from '$lib/api/generated/TlsVersion';
  import { isTauri } from '$lib/api/transport';
  import { t } from '$lib/i18n';
  import { capabilities } from '$lib/platform/capabilities.svelte';
  import Input from '$lib/ui/Input.svelte';
  import Select from '$lib/ui/overlay/Select.svelte';
  import Textarea from '$lib/ui/Textarea.svelte';

  interface Props {
    settings: RequestSettings;
    onchange: (settings: RequestSettings) => void;
  }

  let { settings, onchange }: Props = $props();

  type Flag =
    | 'followRedirects'
    | 'verifyTls'
    | 'encodeUrl'
    | 'sendCookies'
    | 'storeCookies'
    | 'compression'
    | 'keepAlive';
  type Num = 'timeoutMs' | 'connectTimeoutMs' | 'maxRedirects' | 'maxResponseBytes' | 'delayMs';
  type Lines = 'resolve' | 'connectTo';
  type Text = 'unixSocket' | 'interface';
  /** Unix sockets and interfaces only make sense on the user's own machine. */
  const desktop = isTauri();

  const flags: { key: Flag; label: () => string }[] = [
    { key: 'followRedirects', label: () => t('requestSettings.followRedirects') },
    { key: 'verifyTls', label: () => t('requestSettings.verifyTls') },
    { key: 'encodeUrl', label: () => t('requestSettings.encodeUrl') },
    { key: 'sendCookies', label: () => t('requestSettings.sendCookies') },
    { key: 'storeCookies', label: () => t('requestSettings.storeCookies') },
    { key: 'compression', label: () => t('requestSettings.compression') },
    { key: 'keepAlive', label: () => t('requestSettings.keepAlive') },
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
    {
      key: 'maxResponseBytes',
      label: () => t('requestSettings.maxResponseBytes'),
      unit: () => t('requestSettings.bytesZeroNone'),
    },
    {
      key: 'delayMs',
      label: () => t('requestSettings.delay'),
      unit: () => t('requestSettings.ms'),
    },
  ];
  const lines: { key: Lines; label: () => string; placeholder: string }[] = [
    {
      key: 'resolve',
      label: () => t('requestSettings.resolve'),
      placeholder: 'api.example.com:443:10.0.0.5',
    },
    {
      key: 'connectTo',
      label: () => t('requestSettings.connectTo'),
      placeholder: 'api.example.com:443:staging.internal:8443',
    },
  ];
  const texts: { key: Text; label: () => string; placeholder: string }[] = [
    {
      key: 'unixSocket',
      label: () => t('requestSettings.unixSocket'),
      placeholder: '/var/run/docker.sock',
    },
    { key: 'interface', label: () => t('requestSettings.interface'), placeholder: 'eth0' },
  ];
  const httpVersions = $derived<{ value: HttpVersion | 'inherit'; label: string }[]>([
    { value: 'inherit', label: t('requestSettings.inherit') },
    { value: 'auto', label: t('requestSettings.httpAuto') },
    { value: 'http1.1', label: 'HTTP/1.1' },
    { value: 'http2', label: 'HTTP/2' },
    { value: 'http2PriorKnowledge', label: t('requestSettings.http2PriorKnowledge') },
    { value: 'http3', label: 'HTTP/3' },
  ]);
  const tlsVersions = $derived<{ value: TlsVersion | 'inherit'; label: string }[]>([
    { value: 'inherit', label: t('requestSettings.inherit') },
    { value: 'default', label: t('requestSettings.tlsDefault') },
    { value: 'tls1.0', label: 'TLS 1.0' },
    { value: 'tls1.1', label: 'TLS 1.1' },
    { value: 'tls1.2', label: 'TLS 1.2' },
    { value: 'tls1.3', label: 'TLS 1.3' },
  ]);

  function setChoice<K extends 'httpVersion' | 'tlsMin' | 'tlsMax'>(key: K, v: string) {
    onchange({ ...settings, [key]: v === 'inherit' ? null : v });
  }

  function setLines(key: Lines, raw: string) {
    const list = raw
      .split('\n')
      .map((l) => l.trim())
      .filter(Boolean);
    onchange({ ...settings, [key]: list.length ? list : null });
  }

  function setText(key: Text, raw: string) {
    onchange({ ...settings, [key]: raw.trim() ? raw.trim() : null });
  }

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
  <div class="row">
    <span class="label">{t('requestSettings.httpVersion')}</span>
    <Select
      label={t('requestSettings.httpVersion')}
      value={settings.httpVersion ?? 'inherit'}
      options={httpVersions}
      onchange={(v) => setChoice('httpVersion', v)}
    />
  </div>
  <div class="row">
    <span class="label">{t('requestSettings.tlsMin')}</span>
    <Select
      label={t('requestSettings.tlsMin')}
      value={settings.tlsMin ?? 'inherit'}
      options={tlsVersions}
      onchange={(v) => setChoice('tlsMin', v)}
    />
  </div>
  <div class="row">
    <span class="label">{t('requestSettings.tlsMax')}</span>
    <Select
      label={t('requestSettings.tlsMax')}
      value={settings.tlsMax ?? 'inherit'}
      options={tlsVersions}
      onchange={(v) => setChoice('tlsMax', v)}
    />
  </div>
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
  {#each lines as l (l.key)}
    <label class="row top">
      <span class="label">{l.label()}</span>
      <span class="stack">
        <Textarea
          mono
          rows={2}
          value={(settings[l.key] ?? []).join('\n')}
          placeholder={l.placeholder}
          aria-label={l.label()}
          onchange={(e) => setLines(l.key, e.currentTarget.value)}
        />
        <span class="unit">{t('requestSettings.onePerLine')}</span>
      </span>
    </label>
  {/each}
  {#if desktop}
    {#each texts as x (x.key)}
      {@const available = capabilities.has(x.key)}
      <label class="row" class:top={!available}>
        <span class="label">{x.label()}</span>
        <span class="stack">
          <Input
            mono
            value={settings[x.key] ?? ''}
            placeholder={x.placeholder}
            aria-label={x.label()}
            disabled={!available}
            onchange={(e) => setText(x.key, e.currentTarget.value)}
          />
          {#if !available}
            <span class="unit">{t('platform.unavailableAndroid')}</span>
          {/if}
        </span>
      </label>
    {/each}
  {/if}
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
  .row.top {
    align-items: start;
  }
  .stack {
    display: flex;
    flex-direction: column;
    gap: 2px;
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
