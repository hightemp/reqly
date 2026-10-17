// Color design tokens. Each becomes a CSS custom property `--c-<token>`.
// Adding a token here requires adding it to every builtin theme (enforced by tests).

export const COLOR_TOKENS = [
  // Surfaces
  'bg',
  'bg-elevated',
  'bg-sunken',
  'bg-hover',
  'bg-active',
  'bg-selected',
  'overlay',
  'border',
  'border-strong',
  // Text
  'fg',
  'fg-muted',
  'fg-subtle',
  'fg-on-accent',
  // Accent & semantic
  'accent',
  'accent-hover',
  'accent-subtle',
  'focus-ring',
  'danger',
  'danger-subtle',
  'warning',
  'warning-subtle',
  'success',
  'success-subtle',
  'info',
  'info-subtle',
  'shadow',
  'scrollbar-thumb',
  'scrollbar-thumb-hover',
  // HTTP methods
  'method-get',
  'method-post',
  'method-put',
  'method-patch',
  'method-delete',
  'method-head',
  'method-options',
  'method-other',
  // Response status classes
  'status-1xx',
  'status-2xx',
  'status-3xx',
  'status-4xx',
  'status-5xx',
  'status-error',
  // Syntax highlighting (CodeMirror)
  'syntax-keyword',
  'syntax-string',
  'syntax-number',
  'syntax-boolean',
  'syntax-null',
  'syntax-property',
  'syntax-comment',
  'syntax-punctuation',
  'syntax-variable',
  'syntax-variable-missing',
  'syntax-tag',
  'syntax-attribute',
  'syntax-function',
  'syntax-error',
  'syntax-selection',
] as const;

export type ColorToken = (typeof COLOR_TOKENS)[number];

export const cssVar = (token: ColorToken) => `--c-${token}`;
