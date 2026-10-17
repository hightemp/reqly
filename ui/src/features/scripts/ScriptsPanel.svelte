<!-- Pre-request and after-response scripts with API completion and snippets. -->
<script lang="ts">
  import type { Scripts } from '$lib/api/generated/Scripts';
  import { t, type Key } from '$lib/i18n';
  import { SNIPPETS } from '$lib/script/api';
  import { scriptCompletions } from '$lib/script/completion';
  import Button from '$lib/ui/Button.svelte';
  import CodeEditor from '$lib/ui/code/CodeEditor.svelte';
  import { menus } from '$lib/ui/overlay/menus.svelte';
  import Tabs from '$lib/ui/Tabs.svelte';

  interface Props {
    scripts: Scripts;
    onchange: (scripts: Scripts) => void;
    /** Shown above the editors (e.g. where the scripts run). */
    hint?: string;
  }

  let { scripts, onchange, hint }: Props = $props();
  let phase = $state<'pre' | 'post'>('pre');
  let editor: CodeEditor | undefined = $state();

  const value = $derived(phase === 'pre' ? scripts.preRequest : scripts.afterResponse);

  function set(text: string) {
    onchange(
      phase === 'pre' ? { ...scripts, preRequest: text } : { ...scripts, afterResponse: text },
    );
  }

  function insert(code: string) {
    const view = editor?.getView();
    if (!view) return set(value + (value && !value.endsWith('\n') ? '\n' : '') + code);
    const at = view.state.selection.main.head;
    const prefix = at > 0 && view.state.doc.sliceString(at - 1, at) !== '\n' ? '\n' : '';
    view.dispatch({
      changes: { from: at, insert: prefix + code },
      selection: { anchor: at + prefix.length + code.length },
    });
    view.focus();
  }

  function snippetMenu(anchor: HTMLElement) {
    menus.openBelow(
      anchor,
      SNIPPETS.filter((s) => s.phase === 'both' || s.phase === phase).map((s) => ({
        id: s.id,
        label: t(`scripts.snippet.${s.label}` as Key),
        onSelect: () => insert(s.code),
      })),
    );
  }
</script>

<div class="scripts">
  <div class="bar">
    <Tabs
      label={t('scripts.phase')}
      idPrefix="scripts"
      bind:value={phase}
      tabs={[
        { id: 'pre', label: t('scripts.preRequest'), dot: scripts.preRequest.trim() !== '' },
        { id: 'post', label: t('scripts.afterResponse'), dot: scripts.afterResponse.trim() !== '' },
      ]}
    />
    <span class="spacer"></span>
    <Button size="sm" variant="ghost" onclick={(e) => snippetMenu(e.currentTarget)}
      >{t('scripts.snippets')}</Button
    >
  </div>
  {#if hint}<p class="hint">{hint}</p>{/if}
  <div class="editor">
    {#key phase}
      <CodeEditor
        bind:this={editor}
        label={phase === 'pre' ? t('scripts.preRequest') : t('scripts.afterResponse')}
        language="javascript"
        {value}
        completions={[scriptCompletions]}
        placeholder={phase === 'pre' ? t('scripts.prePlaceholder') : t('scripts.postPlaceholder')}
        onchange={set}
      />
    {/key}
  </div>
</div>

<style>
  .scripts {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    height: 100%;
    min-height: 0;
  }
  .bar {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .spacer {
    flex: 1;
  }
  .hint {
    font-size: var(--font-size-xs);
    color: var(--c-fg-subtle);
  }
  .editor {
    flex: 1;
    min-height: 160px;
    border: 1px solid var(--c-border);
    border-radius: var(--radius-sm);
    overflow: hidden;
  }
</style>
