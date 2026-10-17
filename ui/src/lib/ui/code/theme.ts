// CodeMirror theme built from Reqly CSS tokens; switching the app theme
// restyles editors without reconfiguring them.

import { HighlightStyle, syntaxHighlighting } from '@codemirror/language';
import { EditorView } from '@codemirror/view';
import { tags as t } from '@lezer/highlight';

const v = (token: string) => `var(--c-${token})`;

export const reqlyTheme = EditorView.theme({
  '&': {
    color: v('fg'),
    backgroundColor: 'transparent',
    height: '100%',
    fontSize: 'var(--font-size-sm)',
  },
  '.cm-scroller': { fontFamily: 'var(--font-mono)', lineHeight: '1.55' },
  '.cm-content': { caretColor: v('accent'), padding: '6px 0' },
  '&.cm-focused': { outline: 'none' },
  '.cm-cursor, .cm-dropCursor': { borderLeftColor: v('accent') },
  '&.cm-focused .cm-selectionBackground, .cm-selectionBackground, ::selection': {
    backgroundColor: `${v('syntax-selection')} !important`,
  },
  '.cm-activeLine': { backgroundColor: 'transparent' },
  '&.cm-focused .cm-activeLine': { backgroundColor: v('bg-hover') },
  '.cm-gutters': { backgroundColor: 'transparent', color: v('fg-subtle'), border: 'none' },
  '.cm-activeLineGutter': { backgroundColor: 'transparent', color: v('fg-muted') },
  '.cm-foldPlaceholder': { backgroundColor: v('bg-active'), color: v('fg-muted'), border: 'none' },
  '.cm-matchingBracket, .cm-nonmatchingBracket': {
    outline: `1px solid ${v('border-strong')}`,
    backgroundColor: 'transparent',
  },
  '.cm-searchMatch': { backgroundColor: v('warning-subtle'), outline: `1px solid ${v('warning')}` },
  '.cm-searchMatch.cm-searchMatch-selected': { backgroundColor: v('accent-subtle') },
  '.cm-panels': { backgroundColor: v('bg-elevated'), color: v('fg'), borderColor: v('border') },
  '.cm-panels input, .cm-panels button': { color: v('fg') },
  '.cm-textfield': {
    backgroundColor: v('bg-sunken'),
    border: `1px solid ${v('border')}`,
    borderRadius: '4px',
  },
  '.cm-button': {
    backgroundImage: 'none',
    backgroundColor: v('bg-active'),
    border: `1px solid ${v('border')}`,
    borderRadius: '4px',
  },
  '.cm-tooltip': {
    backgroundColor: v('bg-elevated'),
    border: `1px solid ${v('border-strong')}`,
    borderRadius: '6px',
  },
  '.cm-tooltip-autocomplete > ul > li[aria-selected]': {
    backgroundColor: v('accent'),
    color: v('fg-on-accent'),
  },
  '.cm-placeholder': { color: v('fg-subtle') },
});

export const reqlyHighlight = syntaxHighlighting(
  HighlightStyle.define([
    { tag: [t.keyword, t.operatorKeyword, t.modifier], color: v('syntax-keyword') },
    { tag: [t.string, t.special(t.string), t.regexp], color: v('syntax-string') },
    { tag: [t.number, t.integer, t.float], color: v('syntax-number') },
    { tag: t.bool, color: v('syntax-boolean') },
    { tag: t.null, color: v('syntax-null') },
    { tag: [t.propertyName, t.definition(t.propertyName)], color: v('syntax-property') },
    {
      tag: [t.comment, t.lineComment, t.blockComment],
      color: v('syntax-comment'),
      fontStyle: 'italic',
    },
    { tag: [t.punctuation, t.bracket, t.separator], color: v('syntax-punctuation') },
    { tag: [t.variableName, t.self], color: v('syntax-variable') },
    { tag: [t.tagName, t.typeName, t.className], color: v('syntax-tag') },
    { tag: [t.attributeName], color: v('syntax-attribute') },
    { tag: [t.function(t.variableName), t.function(t.propertyName)], color: v('syntax-function') },
    { tag: t.invalid, color: v('syntax-error') },
  ]),
);
