<!-- Method + URL + Send/Cancel. Pasting a cURL command imports it. -->
<script lang="ts">
  import type { KeyValue } from '$lib/api/generated/KeyValue';
  import { t } from '$lib/i18n';
  import Button from '$lib/ui/Button.svelte';
  import IconButton from '$lib/ui/IconButton.svelte';
  import TemplateInput from '$lib/ui/code/TemplateInput.svelte';
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
    oncode?: () => void;
    /** Request whose variables apply. */
    nodeId?: string | null;
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
    oncode,
    nodeId = null,
  }: Props = $props();
  let input: TemplateInput | undefined = $state();

  /** Focuses the URL and selects it (Mod+L). */
  export function focusUrl() {
    input?.focus();
    const view = input?.getView();
    view?.dispatch({ selection: { anchor: 0, head: view.state.doc.length } });
  }

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
  <div class="url">
    <TemplateInput
      bind:this={input}
      value={text}
      label={t('request.url')}
      placeholder={t('request.urlPlaceholder')}
      {nodeId}
      onfocus={() => (focused = true)}
      onblur={() => (focused = false)}
      onchange={(v) => {
        text = v;
        commit(v);
      }}
      onpaste={(pasted) => {
        if (/^\s*curl(\.exe)?\s/i.test(pasted)) {
          oncurl(pasted);
          return true;
        }
        return false;
      }}
      onenter={onsend}
    />
  </div>
  {#if oncode}
    <IconButton icon="braces" label={t('actions.generateCode')} onclick={oncode} />
  {/if}
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
    display: flex;
  }
</style>
