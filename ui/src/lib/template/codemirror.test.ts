import { CompletionContext, type Completion } from '@codemirror/autocomplete';
import { EditorState } from '@codemirror/state';
import { describe, expect, it } from 'vitest';
import { templateCompletions } from './codemirror';
import type { TemplateData, TemplateHost } from './host';

const data: TemplateData = {
  variables: [
    {
      name: 'baseUrl',
      value: 'https://api.test',
      source: 'base',
      sourceName: 'Dev',
      secret: false,
    },
  ],
  functions: [
    { name: 'uuid', args: ['version?'], choices: ['v4', 'v7'], example: "{% uuid 'v4' %}" },
    { name: 'timestamp', args: [], choices: [], example: '{% timestamp %}' },
  ],
  faker: [],
  dynamicVariables: ['randomFirstName'],
};

const host: TemplateHost = {
  load: async () => data,
  onChange: () => () => {},
  preview: async (t) => t,
};

async function complete(doc: string) {
  const state = EditorState.create({ doc });
  const result = await templateCompletions(host)(new CompletionContext(state, doc.length, true));
  return result;
}

const apply = (c: Completion) => (typeof c.apply === 'string' ? c.apply : c.label);

describe('template completion', () => {
  it('offers functions and dynamic variables right after {{', async () => {
    const result = (await complete('x {{ '))!;
    const labels = result.options.map((o) => o.label);
    expect(labels).toEqual(
      expect.arrayContaining(['baseUrl', '$uuid', '$timestamp', '$faker.', '$randomFirstName']),
    );
    // Variables come first.
    const byLabel = (l: string) => result.options.find((o) => o.label === l)!;
    expect(byLabel('baseUrl').boost ?? 0).toBeGreaterThan(byLabel('$uuid').boost ?? 0);
    expect(apply(byLabel('$uuid'))).toBe('$uuid }}');
    expect(apply(byLabel('$randomFirstName'))).toBe('$randomFirstName }}');
    expect(result.from).toBe(5);
  });

  it('keeps the closing braces that are already there', async () => {
    const state = EditorState.create({ doc: '{{ u }}' });
    const result = (await templateCompletions(host)(new CompletionContext(state, 4, true)))!;
    const uuid = result.options.find((o) => o.label === '$uuid')!;
    expect(apply(uuid)).toBe('$uuid');
  });

  it('after {{ $ offers functions without the $', async () => {
    const result = (await complete('{{ $u'))!;
    expect(result.options.map((o) => o.label)).toContain('uuid');
    expect(result.options.map((o) => o.label)).not.toContain('baseUrl');
  });

  it('offers only variables after {{ _.', async () => {
    const result = (await complete('{{ _.'))!;
    expect(result.options.map((o) => o.label)).not.toContain('$uuid');
  });
});
