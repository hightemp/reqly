import { CompletionContext } from '@codemirror/autocomplete';
import { EditorState } from '@codemirror/state';
import { describe, expect, it } from 'vitest';
import { SNIPPETS } from './api';
import { membersAt, scriptCompletions } from './completion';

function complete(text: string) {
  const state = EditorState.create({ doc: text });
  return scriptCompletions(new CompletionContext(state, text.length, false));
}

describe('script completion', () => {
  it('completes nested API members', () => {
    const r = complete('pm.response.to.have.st');
    expect(r?.options.map((o) => o.label)).toContain('status');
    expect(r?.from).toBe('pm.response.to.have.'.length);
    expect(complete('rq.')?.options.map((o) => o.label)).toContain('faker');
    expect(complete('insomnia.')?.options.map((o) => o.label)).toContain('baseEnvironment');
    expect(complete('pm.environment.')?.options.find((o) => o.label === 'set')?.type).toBe(
      'function',
    );
    expect(complete('cons')?.options.map((o) => o.label)).toContain('console');
    expect(complete('unknown.x')).toBeNull();
  });

  it('describes request lists and snippets', () => {
    expect(Object.keys(membersAt(['pm', 'request', 'headers']) ?? {})).toContain('upsert');
    expect(new Set(SNIPPETS.map((s) => s.id)).size).toBe(SNIPPETS.length);
  });
});
