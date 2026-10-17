<!-- Trusted CA certificates and client certificates (mTLS) per host. -->
<script lang="ts">
  import { onMount } from 'svelte';
  import type { ClientCertificate } from '$lib/api/generated/ClientCertificate';
  import type { TlsSettings } from '$lib/api/generated/TlsSettings';
  import { t } from '$lib/i18n';
  import { browserChoose, toBase64 } from '$lib/platform/files';
  import type { Services } from '$lib/services.svelte';
  import Button from '$lib/ui/Button.svelte';
  import Checkbox from '$lib/ui/Checkbox.svelte';
  import IconButton from '$lib/ui/IconButton.svelte';
  import Input from '$lib/ui/Input.svelte';
  import Textarea from '$lib/ui/Textarea.svelte';

  let { services }: { services: Services } = $props();
  const KEY = 'http.tls';
  let tls = $state<TlsSettings | null>(null);
  let pasteOpen = $state(false);

  onMount(async () => {
    const { value } = await services.transport.call('settings.get', { key: KEY });
    const v = (value ?? {}) as Partial<TlsSettings>;
    tls = { caPems: v.caPems ?? [], clientCertificates: v.clientCertificates ?? [] };
  });

  async function save(next: TlsSettings) {
    tls = next;
    await services.transport.call('settings.set', { key: KEY, value: next });
  }

  async function readFile(): Promise<{ text: string; bytes: Uint8Array; name: string } | null> {
    const file = await browserChoose();
    if (!file) return null;
    const bytes = new Uint8Array(await file.arrayBuffer());
    return { text: new TextDecoder().decode(bytes), bytes, name: file.name };
  }

  async function addCa() {
    const file = await readFile();
    if (file && tls) await save({ ...tls, caPems: [...tls.caPems, file.text] });
  }

  function updateCert(i: number, patch: Partial<ClientCertificate>) {
    if (!tls) return;
    void save({
      ...tls,
      clientCertificates: tls.clientCertificates.map((c, j) => (i === j ? { ...c, ...patch } : c)),
    });
  }

  async function loadInto(i: number, field: 'certPem' | 'keyPem' | 'pfxBase64') {
    const file = await readFile();
    if (file) updateCert(i, { [field]: field === 'pfxBase64' ? toBase64(file.bytes) : file.text });
  }

  const subject = (pem: string) =>
    `${pem
      .split('\n')
      .filter((l) => l && !l.startsWith('-----'))
      .join('')
      .slice(0, 24)}…`;
</script>

{#if tls}
  <h3 class="title">{t('certs.ca')}</h3>
  <p class="hint">{t('certs.caHint')}</p>
  {#each tls.caPems as pem, i (i)}
    <div class="row">
      <span class="mono">{subject(pem)}</span>
      <IconButton
        icon="trash"
        size={14}
        label={t('tree.delete')}
        onclick={() => tls && void save({ ...tls, caPems: tls.caPems.filter((_, j) => j !== i) })}
      />
    </div>
  {/each}
  <Button size="sm" onclick={() => void addCa()}>{t('certs.addCa')}</Button>

  <h3 class="title">{t('certs.client')}</h3>
  <p class="hint">{t('certs.clientHint')}</p>
  {#each tls.clientCertificates as cert, i (i)}
    <div class="cert">
      <div class="row">
        <Checkbox
          label={t('kv.enabled')}
          checked={cert.enabled}
          onchange={(v) => updateCert(i, { enabled: v })}
        />
        <Input
          mono
          value={cert.host}
          placeholder={t('certs.hostPlaceholder')}
          aria-label={t('certs.host')}
          onchange={(e) => updateCert(i, { host: e.currentTarget.value.trim() })}
        />
        <IconButton
          icon="trash"
          size={14}
          label={t('tree.delete')}
          onclick={() =>
            tls &&
            void save({
              ...tls,
              clientCertificates: tls.clientCertificates.filter((_, j) => j !== i),
            })}
        />
      </div>
      <div class="files">
        <Button size="sm" onclick={() => void loadInto(i, 'certPem')}
          >{cert.certPem ? t('certs.certLoaded') : t('certs.loadCert')}</Button
        >
        <Button size="sm" onclick={() => void loadInto(i, 'keyPem')}
          >{cert.keyPem ? t('certs.keyLoaded') : t('certs.loadKey')}</Button
        >
        <span class="or">{t('certs.or')}</span>
        <Button size="sm" onclick={() => void loadInto(i, 'pfxBase64')}
          >{cert.pfxBase64 ? t('certs.pfxLoaded') : t('certs.loadPfx')}</Button
        >
        <Input
          type="password"
          value={cert.passphrase}
          placeholder={t('certs.passphrase')}
          aria-label={t('certs.passphrase')}
          onchange={(e) => updateCert(i, { passphrase: e.currentTarget.value })}
        />
      </div>
    </div>
  {/each}
  <Button
    size="sm"
    onclick={() =>
      tls &&
      void save({
        ...tls,
        clientCertificates: [
          ...tls.clientCertificates,
          { enabled: true, host: '', certPem: '', keyPem: '', pfxBase64: '', passphrase: '' },
        ],
      })}
  >
    {t('certs.addClient')}
  </Button>
  <button
    type="button"
    class="paste-toggle"
    aria-expanded={pasteOpen}
    onclick={() => (pasteOpen = !pasteOpen)}>{t('certs.pasteCa')}</button
  >
  {#if pasteOpen}
    <Textarea
      mono
      aria-label={t('certs.pasteCa')}
      placeholder={t('certs.pastePlaceholder')}
      onchange={(e) => {
        const pem = e.currentTarget.value.trim();
        if (pem.includes('BEGIN CERTIFICATE') && tls) {
          void save({ ...tls, caPems: [...tls.caPems, pem] });
          e.currentTarget.value = '';
        }
      }}
    />
  {/if}
{/if}

<style>
  .title {
    font-weight: 600;
    margin: var(--space-3) 0 var(--space-1);
  }
  .hint {
    color: var(--c-fg-subtle);
    margin-bottom: var(--space-2);
  }
  .row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    margin-bottom: var(--space-1);
  }
  .mono {
    flex: 1;
    font-family: var(--font-mono);
    font-size: var(--font-size-sm);
    color: var(--c-fg-muted);
  }
  .cert {
    padding: var(--space-2);
    border: 1px solid var(--c-border);
    border-radius: var(--radius-sm);
    margin-bottom: var(--space-2);
  }
  .files {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex-wrap: wrap;
  }
  .files :global(.input) {
    width: 160px;
  }
  .or {
    color: var(--c-fg-subtle);
  }
  .paste-toggle {
    display: block;
    margin: var(--space-3) 0 var(--space-2);
    color: var(--c-fg-muted);
  }
  .paste-toggle:hover {
    color: var(--c-fg);
  }
</style>
