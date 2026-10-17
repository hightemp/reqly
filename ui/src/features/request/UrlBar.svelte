<!-- Method + URL + Send/Cancel. Pasting a cURL command imports it. -->
<script lang="ts">
  import type { KeyValue } from '$lib/api/generated/KeyValue';
  import { t } from '$lib/i18n';
  import Button from '$lib/ui/Button.svelte';
  import Combobox from '$lib/ui/overlay/Combobox.svelte';
  import { methodColorVar } from '$lib/ui/badges';
  import { COMMON_METHODS } from './headers';
  import { applyUrlInput, displayUrl } from './urlsync';

  interface Props {
    method: string;
    url: string;
    params: KeyValue[];
    sending: boolean;
    onmethod: (method: string) => void;
    onurl: (url: string, params: KeyValue[]) => void;
    onsend: () => void;
    oncancel: () => void;
    oncurl: (command: string) => void;
    input?: HTMLInputElement | null;
  }

  let {
    method,
    url,
    params,
    sending,
    onmethod,
    onurl,
    onsend,
    oncancel,
    oncurl,
    input = $bindable(null),
  }: Props = $props();

  // Local text while focused so re-rendering from params does not move the caret.
  let text = $state('');
  let focused = $state(false);
  $effect(() => {
    if (!focused) text = displayUrl(url, params);
  });

  function commit(value: string) {
    const next = applyUrlInput(value, params, () => crypto.randomUUID());
    onurl(next.url, next.params);
  }
</script>

<div class="urlbar" style:--method-color={methodColorVar(method)}>
  <div class="method">
    <Combobox
      label={t('request.method')}
      value={method}
      options={COMMON_METHODS}
      allowCustom
      transform={(v) => v.toUpperCase()}
      onchange={onmethod}
    />
  </div>
  <input
    bind:this={input}
    class="url"
    value={text}
    spellcheck="false"
    autocomplete="off"
    aria-label={t('request.url')}
    placeholder={t('request.urlPlaceholder')}
    onfocus={() => (focused = true)}
    onblur={() => (focused = false)}
    oninput={(e) => {
      text = e.currentTarget.value;
      commit(text);
    }}
    onpaste={(e) => {
      const pasted = e.clipboardData?.getData('text') ?? '';
      if (/^\s*curl(\.exe)?\s/i.test(pasted)) {
        e.preventDefault();
        oncurl(pasted);
      }
    }}
    onkeydown={(e) => {
      if (e.key === 'Enter' && !e.ctrlKey && !e.metaKey) {
        e.preventDefault();
        onsend();
      }
    }}
  />
  {#if sending}
    <Button variant="default" onclick={oncancel}>{t('request.cancel')}</Button>
  {:else}
    <Button variant="primary" onclick={onsend}>{t('request.send')}</Button>
  {/if}
</div>

<style>
  .urlbar {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-3);
    border-bottom: 1px solid var(--c-border);
  }
  .method {
    width: 104px;
    flex: none;
  }
  .method :global(.combo) {
    font-weight: 700;
    color: var(--method-color);
    font-family: var(--font-mono);
    font-size: var(--font-size-sm);
  }
  .url {
    flex: 1;
    min-width: 0;
    height: var(--control-height);
    padding: 0 var(--space-2);
    border-radius: var(--radius-sm);
    border: 1px solid var(--c-border);
    background: var(--c-bg-sunken);
    color: var(--c-fg);
    font-family: var(--font-mono);
    font-size: var(--font-size-sm);
    outline: none;
  }
  .url:focus {
    border-color: var(--c-accent);
  }
</style>
