import { describe, expect, it } from 'vitest';
import type { FunctionInfo } from '$lib/api/generated/FunctionInfo';
import { decodeFilter, finishTag, openTag, requestOptions, visibleParams } from './tagModel';

const response: FunctionInfo = {
  name: 'response',
  args: ['field', 'requestId', 'path?', 'trigger?', 'maxAgeSeconds?'],
  choices: ['body', 'header', 'raw', 'url', 'status'],
  example: '',
  params: [
    {
      name: 'field',
      kind: 'choice',
      optional: false,
      choices: ['body', 'header', 'raw', 'url', 'status'],
    },
    { name: 'requestId', kind: 'request', optional: false, choices: [] },
    { name: 'path', kind: 'text', optional: true, choices: [] },
    {
      name: 'trigger',
      kind: 'choice',
      optional: true,
      choices: ['never', 'no-history', 'when-expired', 'always'],
    },
    { name: 'maxAgeSeconds', kind: 'number', optional: true, choices: [] },
  ],
};

describe('tag editor model', () => {
  it('decodes Insomnia filters for editing', () => {
    expect(decodeFilter('b64::JC50b2tlbg==::46b')).toBe('$.token');
    expect(decodeFilter('$.id')).toBe('$.id');
    expect(decodeFilter('b64::!!!::46b')).toBe('b64::!!!::46b');
  });

  it('opens an imported Insomnia response tag with a plain filter', () => {
    const tag = openTag("{% response 'body', '01a2', 'b64::JC50b2tlbg==::46b', 'always', 60 %}", [
      response,
    ]);
    expect(tag.name).toBe('response');
    expect(tag.args.map((a) => a.value)).toEqual(['body', '01a2', '$.token', 'always', '60']);
    // A new tag starts with the first choices filled in.
    expect(openTag('', [response]).name).toBe('response');
  });

  it('shows the filter only for body and headers, the age only when it matters', () => {
    const names = (args: string[]) =>
      visibleParams(response, {
        name: 'response',
        args: args.map((value) => ({ kind: 'string', value })),
      }).map((p) => p.name);
    expect(names(['body', 'r', '', 'never'])).toEqual(['field', 'requestId', 'path', 'trigger']);
    expect(names(['status', 'r', '', 'never'])).toEqual(['field', 'requestId', 'trigger']);
    expect(names(['header', 'r', 'X-Token', 'when-expired'])).toEqual([
      'field',
      'requestId',
      'path',
      'trigger',
      'maxAgeSeconds',
    ]);
  });

  it('drops empty optional arguments at the end, keeps numbers as numbers', () => {
    const tag = finishTag(
      {
        name: 'response',
        args: [
          { kind: 'string', value: 'body' },
          { kind: 'string', value: 'r1' },
          { kind: 'string', value: '' },
          { kind: 'string', value: 'when-expired' },
          { kind: 'string', value: '60' },
        ],
      },
      response,
    );
    expect(tag.args).toEqual([
      { kind: 'string', value: 'body' },
      { kind: 'string', value: 'r1' },
      { kind: 'string', value: '' },
      { kind: 'string', value: 'when-expired' },
      { kind: 'number', value: '60' },
    ]);
    expect(
      finishTag(
        {
          name: 'response',
          args: [
            { kind: 'string', value: 'status' },
            { kind: 'string', value: 'r1' },
            { kind: 'string', value: '' },
            { kind: 'string', value: '' },
          ],
        },
        response,
      ).args,
    ).toHaveLength(2);
  });

  it('lists requests with their folder path', () => {
    expect(
      requestOptions([
        { id: 'f', parentId: null, isFolder: true, label: 'Auth' },
        { id: 'r1', parentId: 'f', isFolder: false, label: 'Login', method: 'POST' },
        { id: 'r2', parentId: null, isFolder: false, label: 'Health', method: 'GET' },
      ]),
    ).toEqual([
      { value: 'r1', label: 'Auth / Login', method: 'POST' },
      { value: 'r2', label: 'Health', method: 'GET' },
    ]);
  });
});
