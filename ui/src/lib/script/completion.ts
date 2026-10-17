// Completion of the script API (`rq.`, `pm.`, `insomnia.`, globals).

import type { Completion, CompletionContext, CompletionResult } from '@codemirror/autocomplete';
import { SCRIPT_API, type ApiNode } from './api';

/** Children of the object at `path` (`['pm', 'request']`). */
export function membersAt(path: string[]): Record<string, ApiNode> | null {
  let children: Record<string, ApiNode> | undefined = SCRIPT_API;
  for (const part of path) {
    const node: ApiNode | undefined = children?.[part];
    if (!node?.children) return null;
    children = node.children;
  }
  return children ?? null;
}

function options(members: Record<string, ApiNode>): Completion[] {
  return Object.entries(members).map(([name, node]) => ({
    label: name,
    type: node.kind === 'fn' ? 'function' : node.kind === 'object' ? 'namespace' : 'property',
    detail: node.detail,
    info: node.info,
  }));
}

export function scriptCompletions(ctx: CompletionContext): CompletionResult | null {
  const match = ctx.matchBefore(/[A-Za-z_$][\w$]*(?:\.[\w$]*)*/);
  if (!match || (match.from === match.to && !ctx.explicit)) return null;
  const parts = match.text.split('.');
  const word = parts.pop() ?? '';
  const members = membersAt(parts);
  if (!members) return null;
  // Top level only after a few letters (local variables would compete).
  if (!parts.length && word.length < 1 && !ctx.explicit) return null;
  return {
    from: match.to - word.length,
    options: options(members),
    validFor: /^[\w$]*$/,
  };
}
