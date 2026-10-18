<!--
  CodeMirror 6 editor. The document is created once; external `value`
  changes are applied as transactions so undo history and cursor survive.
-->
<script lang="ts" module>
  export type CodeLanguage = 'json' | 'xml' | 'html' | 'yaml' | 'javascript' | 'text';
</script>

<script lang="ts">
  import {
    autocompletion,
    closeBrackets,
    closeBracketsKeymap,
    type CompletionSource,
  } from '@codemirror/autocomplete';
  import { defaultKeymap, history, historyKeymap, indentWithTab } from '@codemirror/commands';
  import { html } from '@codemirror/lang-html';
  import { javascript } from '@codemirror/lang-javascript';
  import { json } from '@codemirror/lang-json';
  import { xml } from '@codemirror/lang-xml';
  import { yaml } from '@codemirror/lang-yaml';
  import { bracketMatching, foldGutter, foldKeymap, indentOnInput } from '@codemirror/language';
  import { highlightSelectionMatches, search, searchKeymap } from '@codemirror/search';
  import { Compartment, EditorState, type Extension } from '@codemirror/state';
  import {
    EditorView,
    drawSelection,
    highlightActiveLine,
    highlightActiveLineGutter,
    keymap,
    lineNumbers,
    placeholder as placeholderExt,
  } from '@codemirror/view';
  import { onMount } from 'svelte';
  import { yCollab, yUndoManagerKeymap } from 'y-codemirror.next';
  import { collabService, type CollabBinding } from '$lib/collab/collab';
  import { templateCompletions, templateExtension } from '$lib/template/codemirror';
  import { templateHostFactory } from '$lib/template/host';
  import { formatJson, formatXml } from './format';
  import { reqlyHighlight, reqlyTheme } from './theme';

  interface Props {
    value?: string;
    language?: CodeLanguage;
    readonly?: boolean;
    wrap?: boolean;
    lineNumbers?: boolean;
    placeholder?: string;
    label: string;
    /** Highlight and complete `{{ }}` / `{% %}` templates. */
    templates?: boolean;
    /** Request or folder whose variables apply to templates. */
    nodeId?: string | null;
    /** Extra extensions. */
    extensions?: Extension[];
    /** Extra completion sources (kept alongside template completions). */
    completions?: CompletionSource[];
    /** Path of the field in `nodeId`'s data to edit together (ADR-012). */
    collab?: string | null;
    onchange?: (value: string) => void;
  }

  let {
    value = $bindable(''),
    language = 'text',
    readonly = false,
    wrap = false,
    lineNumbers: showLines = true,
    placeholder = '',
    label,
    templates = false,
    nodeId = null,
    extensions = [],
    completions = [],
    collab = null,
    onchange,
  }: Props = $props();

  const factory = templateHostFactory();
  const collabs = collabService();
  /** The shared document while the field is edited together. */
  let binding: CollabBinding | null = null;
  let destroyed = false;
  let host: HTMLDivElement | null = $state(null);
  let view: EditorView | null = null;
  const lang = new Compartment();
  const editable = new Compartment();
  const wrapping = new Compartment();

  function languageExtension(l: CodeLanguage): Extension {
    switch (l) {
      case 'json':
        return json();
      case 'xml':
        return xml();
      case 'html':
        return html();
      case 'yaml':
        return yaml();
      case 'javascript':
        return javascript();
      default:
        return [];
    }
  }

  export function getView() {
    return view;
  }

  export function focus() {
    view?.focus();
  }

  /** Pretty-prints JSON/XML content; returns false when it cannot. */
  export function format(): boolean {
    if (!view || readonly) return false;
    const text = view.state.doc.toString();
    const formatted =
      language === 'json'
        ? formatJson(text)
        : language === 'xml' || language === 'html'
          ? formatXml(text)
          : null;
    if (formatted === null || formatted === text) return formatted !== null;
    view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: formatted } });
    return true;
  }

  /** (Re)creates the view, bound to `shared` when editing together. */
  function create(shared: CollabBinding | null) {
    if (destroyed || !host) return;
    view?.destroy();
    const templateHost = templates ? factory?.(nodeId) : undefined;
    view = new EditorView({
      parent: host,
      state: EditorState.create({
        doc: shared ? shared.ytext.toString() : value,
        extensions: [
          showLines ? [lineNumbers(), foldGutter(), highlightActiveLineGutter()] : [],
          shared
            ? yCollab(shared.ytext, shared.awareness, { undoManager: shared.undoManager })
            : history(),
          drawSelection(),
          indentOnInput(),
          bracketMatching(),
          closeBrackets(),
          templateHost
            ? [
                templateExtension(templateHost),
                autocompletion({ override: [templateCompletions(templateHost), ...completions] }),
              ]
            : completions.length
              ? autocompletion({ override: completions })
              : autocompletion(),
          highlightActiveLine(),
          highlightSelectionMatches(),
          search({ top: true }),
          keymap.of([
            ...closeBracketsKeymap,
            ...defaultKeymap,
            ...searchKeymap,
            ...(shared ? yUndoManagerKeymap : historyKeymap),
            ...foldKeymap,
            indentWithTab,
          ]),
          reqlyTheme,
          reqlyHighlight,
          placeholder ? placeholderExt(placeholder) : [],
          lang.of(languageExtension(language)),
          editable.of([EditorState.readOnly.of(readonly), EditorView.editable.of(!readonly)]),
          wrapping.of(wrap ? EditorView.lineWrapping : []),
          EditorView.contentAttributes.of({ 'aria-label': label }),
          EditorView.updateListener.of((u) => {
            if (!u.docChanged) return;
            const next = u.state.doc.toString();
            if (next !== value) {
              value = next;
              onchange?.(next);
            }
          }),
          ...extensions,
        ],
      }),
    });
  }

  onMount(() => {
    create(null);
    return () => {
      destroyed = true;
      binding?.destroy();
      view?.destroy();
    };
  });

  // Edit together on a multi-user server; join again when the document
  // went stale on the server.
  $effect(() => {
    const id = nodeId;
    const field = collab;
    if (!collabs?.enabled || !id || !field) return;
    let cancelled = false;
    const start = () => {
      collabs
        .bind(id, field, () => {
          binding = null;
          if (!cancelled) start();
        })
        .then((b) => {
          if (cancelled) return b.destroy();
          binding = b;
          create(b);
        })
        .catch((e: unknown) => console.warn('[collab] join failed', e));
    };
    start();
    return () => {
      cancelled = true;
      binding?.destroy();
      binding = null;
      create(null);
    };
  });

  // External value changes (e.g. switching requests, cURL import); a shared
  // document brings them itself.
  $effect(() => {
    const next = value;
    if (view && !binding && next !== view.state.doc.toString()) {
      view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: next } });
    }
  });

  $effect(() => {
    view?.dispatch({ effects: lang.reconfigure(languageExtension(language)) });
  });

  $effect(() => {
    view?.dispatch({
      effects: editable.reconfigure([
        EditorState.readOnly.of(readonly),
        EditorView.editable.of(!readonly),
      ]),
    });
  });

  $effect(() => {
    view?.dispatch({ effects: wrapping.reconfigure(wrap ? EditorView.lineWrapping : []) });
  });
</script>

<div bind:this={host} class="code-editor"></div>

<style>
  .code-editor {
    height: 100%;
    min-height: 0;
    overflow: hidden;
  }
  .code-editor :global(.cm-editor) {
    height: 100%;
  }
</style>
