<!--
  Single-line input with template support (highlighting, preview on hover,
  completion after `{{` / `{%`). Works as a plain one-line editor when no
  template host is provided.
-->
<script lang="ts">
  import { autocompletion, completionKeymap, completionStatus } from '@codemirror/autocomplete';
  import { defaultKeymap, history, historyKeymap } from '@codemirror/commands';
  import { EditorState, Prec } from '@codemirror/state';
  import { EditorView, keymap, placeholder as placeholderExt } from '@codemirror/view';
  import { onMount } from 'svelte';
  import { templateCompletions, templateExtension } from '$lib/template/codemirror';
  import { templateHostFactory } from '$lib/template/host';
  import { reqlyTheme } from './theme';

  interface Props {
    value?: string;
    label: string;
    placeholder?: string;
    /** Request or folder whose variables apply. */
    nodeId?: string | null;
    readonly?: boolean;
    /** Monospace text (URLs, header values). */
    mono?: boolean;
    /** Hides the value (secrets, passwords) until focused. */
    masked?: boolean;
    onchange?: (value: string) => void;
    onenter?: () => void;
    /** Return true to cancel the default paste. */
    onpaste?: (text: string) => boolean;
    onfocus?: () => void;
    onblur?: () => void;
  }

  let {
    value = $bindable(''),
    label,
    placeholder = '',
    nodeId = null,
    readonly = false,
    mono = true,
    masked = false,
    onchange,
    onenter,
    onpaste,
    onfocus,
    onblur,
  }: Props = $props();

  const factory = templateHostFactory();
  let host: HTMLDivElement | null = $state(null);
  let view: EditorView | null = null;
  let focused = $state(false);

  export function focus() {
    view?.focus();
  }

  export function getView() {
    return view;
  }

  onMount(() => {
    const templates = factory?.(nodeId);
    view = new EditorView({
      parent: host!,
      state: EditorState.create({
        doc: value,
        extensions: [
          history(),
          // One line: newlines from typing or pasting become nothing.
          EditorState.transactionFilter.of((tr) =>
            tr.newDoc.lines > 1
              ? [
                  tr,
                  {
                    changes: {
                      from: 0,
                      to: tr.newDoc.length,
                      insert: tr.newDoc.sliceString(0).replace(/\r?\n/g, ''),
                    },
                    sequential: true,
                  },
                ]
              : tr,
          ),
          Prec.highest(
            keymap.of([
              {
                key: 'Enter',
                run: (view) => {
                  // Let completion accept the selected option first, also while it
                  // is still updating after the last keystroke.
                  if (completionStatus(view.state) !== null) return false;
                  onenter?.();
                  return true;
                },
              },
            ]),
          ),
          templates
            ? [
                templateExtension(templates),
                autocompletion({ override: [templateCompletions(templates)], icons: false }),
              ]
            : [],
          keymap.of([...completionKeymap, ...defaultKeymap, ...historyKeymap]),
          reqlyTheme,
          singleLineTheme,
          placeholder ? placeholderExt(placeholder) : [],
          EditorState.readOnly.of(readonly),
          EditorView.editable.of(!readonly),
          EditorView.contentAttributes.of({
            'aria-label': label,
            'aria-multiline': 'false',
            spellcheck: 'false',
          }),
          EditorView.domEventHandlers({
            paste(e) {
              const text = e.clipboardData?.getData('text') ?? '';
              if (onpaste?.(text)) {
                e.preventDefault();
                return true;
              }
              return false;
            },
            focus() {
              focused = true;
              onfocus?.();
            },
            blur() {
              focused = false;
              onblur?.();
            },
          }),
          EditorView.updateListener.of((u) => {
            if (!u.docChanged) return;
            const next = u.state.doc.toString();
            if (next !== value) {
              value = next;
              onchange?.(next);
            }
          }),
        ],
      }),
    });
    return () => view?.destroy();
  });

  $effect(() => {
    const next = value;
    if (view && next !== view.state.doc.toString()) {
      view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: next } });
    }
  });

  const singleLineTheme = EditorView.theme({
    '&': { height: 'auto' },
    '.cm-content': { padding: '0' },
    '.cm-line': { padding: '0' },
    '.cm-scroller': { overflow: 'hidden', lineHeight: 'calc(var(--control-height) - 2px)' },
    '&.cm-focused .cm-activeLine': { backgroundColor: 'transparent' },
  });
</script>

<div
  bind:this={host}
  class="template-input"
  class:mono
  class:readonly
  class:masked={masked && !focused && value !== ''}
></div>

<style>
  .template-input {
    flex: 1;
    min-width: 0;
    height: var(--control-height);
    padding: 0 var(--space-2);
    border-radius: var(--radius-sm);
    border: 1px solid var(--c-border);
    background: var(--c-bg-sunken);
    overflow: hidden;
  }
  .template-input:focus-within {
    border-color: var(--c-accent);
  }
  .template-input :global(.cm-editor) {
    font-size: var(--font-size-sm);
  }
  .template-input:not(.mono) :global(.cm-scroller) {
    font-family: var(--font-ui);
  }
  .masked :global(.cm-content) {
    -webkit-text-security: disc;
  }
</style>
