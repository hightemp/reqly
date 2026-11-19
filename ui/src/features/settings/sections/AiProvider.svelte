<!-- One AI provider in the settings: name, base URL, key, extra headers,
     connection check, removal. -->
<script lang="ts">
  import type { AiHeader } from '$lib/api/generated/AiHeader';
  import { errorMessage } from '$lib/errors';
  import { t } from '$lib/i18n';
  import type { AiProviderState, AiStore } from '$lib/stores/ai.svelte';
  import Button from '$lib/ui/Button.svelte';
  import IconButton from '$lib/ui/IconButton.svelte';
  import Input from '$lib/ui/Input.svelte';
  import { dialogs } from '$lib/ui/overlay/dialogs.svelte';
  import { toasts } from '$lib/ui/overlay/toasts.svelte';

  let { ai, provider }: { ai: AiStore; provider: AiProviderState } = $props();

  // svelte-ignore state_referenced_locally
  let name = $state(provider.name);
  // svelte-ignore state_referenced_locally
  let baseUrl = $state(provider.baseUrl);
  // svelte-ignore state_referenced_locally
  let headers = $state<AiHeader[]>(provider.headers.map((h) => ({ ...h })));
  let key = $state('');
  let checking = $state(false);
  // i18n-dynamic: ai.preset.

  $effect(() => {
    name = provider.name;
    baseUrl = provider.baseUrl;
    headers = provider.headers.map((h) => ({ ...h }));
  });

  async function run(action: () => Promise<unknown>) {
    try {
      await action();
    } catch (e) {
      toasts.error(errorMessage(e));
    }
  }

  function saveName() {
    if (name.trim() && name !== provider.name) {
      void run(() => ai.updateProvider(provider.id, { name: name.trim() }));
    }
  }

  function saveUrl() {
    if (baseUrl.trim() !== provider.baseUrl) {
      void run(async () => {
        try {
          await ai.updateProvider(provider.id, { baseUrl: baseUrl.trim() });
        } catch (e) {
          baseUrl = provider.baseUrl;
          throw e;
        }
      });
    }
  }

  function saveHeaders() {
    const clean = headers.filter((h) => h.name.trim());
    if (JSON.stringify(clean) !== JSON.stringify(provider.headers)) {
      void run(() => ai.updateProvider(provider.id, { headers: clean }));
    }
  }

  function saveKey() {
    if (!key.trim()) return;
    void run(async () => {
      await ai.setKey(provider.id, key.trim());
      key = '';
    });
  }

  async function check() {
    checking = true;
    try {
      const models = await ai.loadModels(provider.id);
      toasts.show({ message: t('ai.checkOk', { count: models.length }), kind: 'success' });
    } catch (e) {
      toasts.error(errorMessage(e));
    } finally {
      checking = false;
    }
  }

  async function remove() {
    const ok = await dialogs.confirm({
      title: t('ai.removeTitle', { name: provider.name }),
      message: t('ai.removeMessage'),
      confirmLabel: t('ai.removeProvider'),
      danger: true,
    });
    if (ok) await run(() => ai.removeProvider(provider.id));
  }
</script>

<article class="provider" aria-label={provider.name}>
  <header>
    <span class="preset">{t(`ai.preset.${provider.preset}`)}</span>
    <IconButton
      icon="trash"
      size={14}
      label={t('ai.removeProvider')}
      onclick={() => void remove()}
    />
  </header>
  <label class="row">
    <span>{t('ai.name')}</span>
    <Input bind:value={name} onblur={saveName} />
  </label>
  <label class="row">
    <span>{t('ai.baseUrl')}</span>
    <Input bind:value={baseUrl} mono onblur={saveUrl} spellcheck="false" />
  </label>
  <div class="row">
    <span>{t('ai.key')}</span>
    <div class="key">
      {#if provider.hasKey}
        <span class="badge ok">{t('ai.keySet')}</span>
        <Button
          size="sm"
          variant="ghost"
          onclick={() => void run(() => ai.setKey(provider.id, null))}>{t('ai.clearKey')}</Button
        >
      {:else}
        <span class="badge">{provider.keyRequired ? t('ai.keyNone') : t('ai.keyOptional')}</span>
      {/if}
      <Input
        type="password"
        bind:value={key}
        placeholder={t('ai.keyPlaceholder')}
        aria-label={t('ai.key')}
        autocomplete="off"
        onkeydown={(e: KeyboardEvent) => {
          if (e.key === 'Enter') saveKey();
        }}
      />
      <Button size="sm" disabled={!key.trim()} onclick={saveKey}>{t('ai.saveKey')}</Button>
    </div>
  </div>
  <div class="row top">
    <span>{t('ai.headers')}</span>
    <div class="headers">
      {#each headers as header, i (i)}
        <div class="header">
          <Input
            bind:value={header.name}
            placeholder={t('ai.headerName')}
            aria-label={t('ai.headerName')}
            mono
            onblur={saveHeaders}
          />
          <Input
            bind:value={header.value}
            placeholder={t('ai.headerValue')}
            aria-label={t('ai.headerValue')}
            mono
            onblur={saveHeaders}
          />
          <IconButton
            icon="close"
            size={12}
            label={t('ai.removeHeader')}
            onclick={() => {
              headers = headers.filter((_, j) => j !== i);
              saveHeaders();
            }}
          />
        </div>
      {/each}
      <Button
        size="sm"
        variant="ghost"
        onclick={() => (headers = [...headers, { name: '', value: '' }])}
        >{t('ai.addHeader')}</Button
      >
    </div>
  </div>
  <footer>
    <Button size="sm" loading={checking} onclick={() => void check()}>{t('ai.check')}</Button>
  </footer>
</article>

<style>
  .provider {
    border: 1px solid var(--c-border);
    border-radius: var(--radius-md, 6px);
    padding: var(--space-2) var(--space-3);
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }
  header,
  footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  footer {
    justify-content: flex-end;
  }
  .preset {
    font-weight: 600;
  }
  .row {
    display: grid;
    grid-template-columns: 120px 1fr;
    gap: var(--space-2);
    align-items: center;
  }
  .row.top {
    align-items: start;
  }
  .row > span {
    color: var(--c-fg-muted);
    font-size: var(--font-size-sm);
  }
  .key,
  .header {
    display: flex;
    gap: var(--space-2);
    align-items: center;
    min-width: 0;
  }
  .headers {
    display: flex;
    flex-direction: column;
    gap: var(--space-1, 4px);
    align-items: flex-start;
  }
  .header {
    width: 100%;
  }
  .badge {
    font-size: var(--font-size-xs);
    color: var(--c-fg-subtle);
    white-space: nowrap;
  }
  .badge.ok {
    color: var(--c-success);
  }
</style>
