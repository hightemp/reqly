// CodeMirror support for templates: highlighted "pills" (defined / missing),
// a hover preview of the rendered value and completion after `{{` / `{%`.

import type { Completion, CompletionContext, CompletionResult } from '@codemirror/autocomplete';
import { RangeSetBuilder, type Extension } from '@codemirror/state';
import {
  Decoration,
  EditorView,
  ViewPlugin,
  hoverTooltip,
  type DecorationSet,
  type ViewUpdate,
} from '@codemirror/view';
import { t } from '$lib/i18n';
import { isKnownFunction, type TemplateData, type TemplateHost } from './host';
import { completionContext, scanTemplates, type TemplateRange } from './scan';

// i18n-dynamic: template.source.
const SECRET_MASK = '••••••';

function rangeState(r: TemplateRange, data: TemplateData | null): 'ok' | 'missing' | 'unknown' {
  if (!data) return 'unknown';
  if (r.kind === 'variable')
    return data.variables.some((v) => v.name === r.root) ? 'ok' : 'missing';
  return isKnownFunction(r.name, data.functions) ? 'ok' : 'missing';
}

function decorations(view: EditorView, data: TemplateData | null): DecorationSet {
  const builder = new RangeSetBuilder<Decoration>();
  for (const { from, to } of view.visibleRanges) {
    const text = view.state.doc.sliceString(from, to);
    for (const r of scanTemplates(text)) {
      const state = rangeState(r, data);
      builder.add(
        from + r.from,
        from + r.to,
        Decoration.mark({
          class: `cm-tpl cm-tpl-${r.kind === 'variable' ? 'var' : 'fn'} cm-tpl-${state}`,
          attributes: { 'data-template': r.kind },
        }),
      );
    }
  }
  return builder.finish();
}

function rangeAt(view: EditorView, pos: number): TemplateRange | null {
  const line = view.state.doc.lineAt(pos);
  const hit = scanTemplates(line.text).find(
    (r) => line.from + r.from <= pos && pos <= line.from + r.to,
  );
  return hit ? { ...hit, from: line.from + hit.from, to: line.from + hit.to } : null;
}

const valueText = (v: unknown) => (typeof v === 'string' ? v : JSON.stringify(v));

export function templateExtension(host: TemplateHost): Extension {
  let data: TemplateData | null = null;

  const plugin = ViewPlugin.fromClass(
    class {
      decorations: DecorationSet;
      private off: () => void;
      private alive = true;

      constructor(readonly view: EditorView) {
        this.decorations = decorations(view, data);
        const refresh = () =>
          void host.load().then((d) => {
            data = d;
            if (!this.alive) return;
            this.decorations = decorations(this.view, data);
            // An empty transaction makes CodeMirror re-read plugin decorations.
            this.view.dispatch({});
          });
        refresh();
        this.off = host.onChange(refresh);
      }

      update(u: ViewUpdate) {
        if (u.docChanged || u.viewportChanged) this.decorations = decorations(u.view, data);
      }

      destroy() {
        this.alive = false;
        this.off();
      }
    },
    { decorations: (p) => p.decorations },
  );

  const hover = hoverTooltip((view, pos) => {
    const r = rangeAt(view, pos);
    if (!r) return null;
    return {
      pos: r.from,
      end: r.to,
      above: true,
      create() {
        const dom = document.createElement('div');
        dom.className = 'cm-tpl-preview';
        const value = document.createElement('div');
        value.className = 'cm-tpl-preview-value';
        value.textContent = '…';
        dom.append(value);
        const variable =
          r.kind === 'variable' ? data?.variables.find((v) => v.name === r.root) : null;
        if (variable) {
          const source = document.createElement('div');
          source.className = 'cm-tpl-preview-source';
          source.textContent = `${t(`template.source.${variable.source}`)}: ${variable.sourceName}`;
          dom.append(source);
        }
        if (variable?.secret && r.name === r.root) {
          value.textContent = SECRET_MASK;
        } else {
          host.preview(r.text).then(
            (v) => (value.textContent = v === '' ? t('template.empty') : v),
            (e: unknown) => {
              value.classList.add('error');
              value.textContent = e instanceof Error ? e.message : String(e);
            },
          );
        }
        if (r.kind !== 'variable' && host.editTag) {
          const edit = document.createElement('button');
          edit.type = 'button';
          edit.className = 'cm-tpl-edit';
          edit.textContent = t('template.editTag');
          edit.onclick = () =>
            host.editTag!(r.text, (next) =>
              view.dispatch({ changes: { from: r.from, to: r.to, insert: next } }),
            );
          dom.append(edit);
        }
        return { dom };
      },
    };
  });

  const clickToEdit = EditorView.domEventHandlers({
    mousedown(e, view) {
      if (!(e.ctrlKey || e.metaKey) || !host.editTag) return false;
      const pos = view.posAtCoords({ x: e.clientX, y: e.clientY });
      const r = pos === null ? null : rangeAt(view, pos);
      if (!r || r.kind === 'variable') return false;
      e.preventDefault();
      host.editTag(r.text, (next) =>
        view.dispatch({ changes: { from: r.from, to: r.to, insert: next } }),
      );
      return true;
    },
  });

  return [plugin, hover, clickToEdit, templateTheme];
}

/** Completion source for `{{ var }}`, `{{ $fn }}`, `{{ $faker.x }}` and `{% tag %}`. */
export function templateCompletions(host: TemplateHost) {
  return async (ctx: CompletionContext): Promise<CompletionResult | null> => {
    const line = ctx.state.doc.lineAt(ctx.pos);
    const before = line.text.slice(0, ctx.pos - line.from);
    const at = completionContext(before);
    if (!at) return null;
    const data = await host.load();
    const after = line.text.slice(ctx.pos - line.from);
    const closed = (close: string) => new RegExp(`^[^{]*${close}`).test(after);
    const from = line.from + at.from;
    let options: Completion[];
    if (at.kind === 'variable') {
      options = data.variables.map((v) => ({
        label: v.name,
        type: 'variable',
        detail: `${t(`template.source.${v.source}`)} · ${v.sourceName}`,
        info: v.secret ? SECRET_MASK : valueText(v.value),
        apply: closed('\\}\\}') ? v.name : `${v.name} }}`,
      }));
    } else if (at.kind === 'function') {
      options = [
        ...data.functions.map((f) => ({
          label: f.name,
          type: 'function',
          detail: f.args.join(', '),
          info: f.example,
        })),
        { label: 'faker.', type: 'namespace', detail: 'faker', boost: 1 },
      ];
    } else if (at.kind === 'faker') {
      options = data.faker.map((f) => ({
        label: f.name,
        type: 'function',
        detail: f.args,
        // A sample value, generated on demand.
        info: () =>
          host.preview(`{{ $faker.${f.name} }}`).then(
            (v) => {
              const dom = document.createElement('div');
              dom.className = 'cm-tpl-sample';
              dom.textContent = `${t('template.example')}: ${v}`;
              return dom;
            },
            () => null,
          ),
        apply: closed('\\}\\}') ? f.name : `${f.name} }}`,
      }));
    } else {
      options = data.functions.map((f) => ({
        label: f.name,
        type: 'keyword',
        detail: f.args.join(', '),
        info: f.example,
        apply: closed('%\\}') ? f.name : f.example.replace(/^\{%\s*/, ''),
      }));
    }
    return { from, options, validFor: /^[\w.$-]*$/ };
  };
}

const templateTheme = EditorView.baseTheme({
  '.cm-tpl': {
    borderRadius: '4px',
    padding: '0 1px',
  },
  '.cm-tpl-var.cm-tpl-ok': {
    backgroundColor: 'var(--c-accent-subtle)',
    color: 'var(--c-accent)',
  },
  '.cm-tpl-fn.cm-tpl-ok': {
    backgroundColor: 'var(--c-success-subtle)',
    color: 'var(--c-success)',
  },
  '.cm-tpl-missing': {
    backgroundColor: 'var(--c-danger-subtle)',
    color: 'var(--c-danger)',
  },
  '.cm-tpl-unknown': {
    backgroundColor: 'var(--c-bg-active)',
  },
  '.cm-tpl-preview': {
    padding: '6px 8px',
    maxWidth: '420px',
    fontFamily: 'var(--font-mono)',
    fontSize: 'var(--font-size-sm)',
    display: 'flex',
    flexDirection: 'column',
    gap: '4px',
  },
  '.cm-tpl-preview-value': { whiteSpace: 'pre-wrap', wordBreak: 'break-all' },
  '.cm-tpl-preview-value.error': { color: 'var(--c-danger)' },
  '.cm-tpl-preview-source': {
    color: 'var(--c-fg-muted)',
    fontFamily: 'var(--font-ui)',
    fontSize: 'var(--font-size-xs)',
  },
  '.cm-tpl-edit': {
    alignSelf: 'flex-start',
    padding: '2px 8px',
    borderRadius: '4px',
    border: '1px solid var(--c-border)',
    backgroundColor: 'var(--c-bg-active)',
    color: 'var(--c-fg)',
    fontFamily: 'var(--font-ui)',
    cursor: 'pointer',
  },
});
