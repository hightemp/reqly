<script lang="ts">
  import type { Auth } from '$lib/api/generated/Auth';
  import type { OAuth2Config } from '$lib/api/generated/OAuth2Config';
  import { isTauri, type Transport } from '$lib/api/transport';
  import { t, type Key } from '$lib/i18n';
  import Checkbox from '$lib/ui/Checkbox.svelte';
  import TemplateInput from '$lib/ui/code/TemplateInput.svelte';
  import Input from '$lib/ui/Input.svelte';
  import Select from '$lib/ui/overlay/Select.svelte';
  import Textarea from '$lib/ui/Textarea.svelte';
  import OAuth2Token from './auth/OAuth2Token.svelte';
  import {
    type AuthKind,
    defaults,
    DESKTOP_ONLY,
    FIELDS,
    type FieldSpec,
    KINDS,
  } from './auth/schemes';

  interface Props {
    auth: Auth;
    allowInherit?: boolean;
    /** Request or folder whose variables apply (and owns OAuth 2.0 tokens). */
    nodeId?: string | null;
    /** Needed for OAuth 2.0 token actions. */
    transport?: Transport | null;
    onchange: (auth: Auth) => void;
  }

  let { auth, allowInherit = true, nodeId = null, transport = null, onchange }: Props = $props();
  let reveal = $state(false);
  const desktop = isTauri();

  const label = (key: string) => t(`auth.${key}` as Key);
  const kinds = $derived([
    ...(allowInherit ? [{ value: 'inherit' as AuthKind, label: t('auth.inherit') }] : []),
    ...KINDS.filter((k) => desktop || !DESKTOP_ONLY.includes(k.kind) || k.kind === auth.type).map(
      (k) => ({ value: k.kind, label: label(k.label) }),
    ),
  ]);
  const values = $derived(auth as unknown as Record<string, unknown>);
  const fields = $derived((FIELDS[auth.type] ?? []).filter((f) => !f.when || f.when(values)));
  const hasSecrets = $derived(fields.some((f) => f.kind === 'secret'));

  function setKind(kind: AuthKind) {
    if (kind !== auth.type) onchange(defaults(kind, auth));
  }

  function set(key: string, value: unknown) {
    onchange({ ...auth, [key]: value } as Auth);
  }

  function setNumber(key: string, raw: string) {
    const n = Math.max(0, Math.floor(Number(raw)));
    set(key, Number.isFinite(n) ? n : 0);
  }

  function options(f: FieldSpec) {
    return (f.options ?? []).map((o) => ({
      value: o.value,
      label: o.literal ? o.label : label(o.label),
    }));
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
  {:else if DESKTOP_ONLY.includes(auth.type) && !desktop}
    <p class="hint">{t('auth.desktopOnly')}</p>
  {/if}

  {#each fields as f (f.key)}
    {#if f.kind === 'flag'}
      <div class="row">
        <span></span>
        <Checkbox
          checked={Boolean(values[f.key])}
          label={label(f.label)}
          showLabel
          onchange={(v) => set(f.key, v)}
        />
      </div>
    {:else if f.kind === 'select'}
      <label class="row">
        <span class="label">{label(f.label)}</span>
        <Select
          label={label(f.label)}
          value={String(values[f.key])}
          options={options(f)}
          onchange={(v) => set(f.key, v)}
        />
      </label>
    {:else if f.kind === 'number'}
      <label class="row">
        <span class="label">{label(f.label)}</span>
        <Input
          type="number"
          min="0"
          class="number"
          value={String(values[f.key] ?? '')}
          aria-label={label(f.label)}
          onchange={(e) => setNumber(f.key, e.currentTarget.value)}
        />
      </label>
    {:else if f.kind === 'multiline'}
      <label class="row top">
        <span class="label">{label(f.label)}</span>
        <Textarea
          mono
          rows={4}
          value={String(values[f.key] ?? '')}
          placeholder={f.placeholder ?? ''}
          aria-label={label(f.label)}
          onchange={(e) => set(f.key, e.currentTarget.value)}
        />
      </label>
    {:else}
      <div class="row">
        <span class="label">{label(f.label)}</span>
        <TemplateInput
          value={String(values[f.key] ?? '')}
          label={label(f.label)}
          placeholder={f.placeholder ?? ''}
          masked={f.kind === 'secret' && !reveal}
          {nodeId}
          onchange={(v) => set(f.key, v)}
        />
      </div>
    {/if}
  {/each}

  {#if hasSecrets}
    <button type="button" class="reveal" onclick={() => (reveal = !reveal)}
      >{reveal ? t('auth.hide') : t('auth.show')}</button
    >
  {/if}

  {#if auth.type === 'oauth2' && transport && nodeId}
    <OAuth2Token {transport} {nodeId} config={auth as unknown as OAuth2Config} />
  {:else if auth.type === 'oauth2'}
    <p class="hint">{t('auth.token2.saveFirst')}</p>
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
    grid-template-columns: 160px 1fr;
    align-items: center;
    gap: var(--space-3);
  }
  .row.top {
    align-items: start;
  }
  .label {
    color: var(--c-fg-muted);
  }
  .hint {
    color: var(--c-fg-subtle);
  }
  .auth :global(.number) {
    width: 160px;
  }
  .reveal {
    align-self: flex-start;
    margin-left: calc(160px + var(--space-3));
    color: var(--c-fg-muted);
    font-size: var(--font-size-xs);
  }
</style>
