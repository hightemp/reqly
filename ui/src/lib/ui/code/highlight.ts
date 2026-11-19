// Static syntax highlighting for code shown outside an editor (AI chat
// answers): the CodeMirror parsers, rendered as `<span class="hl-*">`.
// The classes are styled in `highlight.css` with the editor's colors.

import { htmlLanguage } from '@codemirror/lang-html';
import { javascriptLanguage, typescriptLanguage } from '@codemirror/lang-javascript';
import { jsonLanguage } from '@codemirror/lang-json';
import { xmlLanguage } from '@codemirror/lang-xml';
import { yamlLanguage } from '@codemirror/lang-yaml';
import type { Language } from '@codemirror/language';
import { highlightCode, tagHighlighter, tags as t } from '@lezer/highlight';

const LANGUAGES: Record<string, Language> = {
  json: jsonLanguage,
  js: javascriptLanguage,
  javascript: javascriptLanguage,
  jsx: javascriptLanguage,
  ts: typescriptLanguage,
  typescript: typescriptLanguage,
  html: htmlLanguage,
  xml: xmlLanguage,
  svg: xmlLanguage,
  yaml: yamlLanguage,
  yml: yamlLanguage,
};

const highlighter = tagHighlighter([
  { tag: [t.keyword, t.operatorKeyword, t.modifier], class: 'hl-keyword' },
  { tag: [t.string, t.special(t.string), t.regexp], class: 'hl-string' },
  { tag: [t.number, t.integer, t.float], class: 'hl-number' },
  { tag: t.bool, class: 'hl-boolean' },
  { tag: t.null, class: 'hl-null' },
  { tag: [t.propertyName, t.definition(t.propertyName)], class: 'hl-property' },
  { tag: [t.comment, t.lineComment, t.blockComment], class: 'hl-comment' },
  { tag: [t.punctuation, t.bracket, t.separator], class: 'hl-punctuation' },
  { tag: [t.variableName, t.self], class: 'hl-variable' },
  { tag: [t.tagName, t.typeName, t.className], class: 'hl-tag' },
  { tag: t.attributeName, class: 'hl-attribute' },
  { tag: [t.function(t.variableName), t.function(t.propertyName)], class: 'hl-function' },
]);

const escape = (s: string) =>
  s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');

/** Highlighted HTML of `code`, or `null` when the language is unknown. */
export function highlightToHtml(code: string, lang: string): string | null {
  const language = LANGUAGES[lang.toLowerCase()];
  if (!language) return null;
  let out = '';
  highlightCode(
    code,
    language.parser.parse(code),
    highlighter,
    (text, classes) => {
      out += classes ? `<span class="${classes}">${escape(text)}</span>` : escape(text);
    },
    () => {
      out += '\n';
    },
  );
  return out;
}
