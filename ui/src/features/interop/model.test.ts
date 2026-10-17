import { describe, expect, it } from 'vitest';
import type { ImportBundle } from '$lib/api/generated/ImportBundle';
import { previewRows, reportText, summarize } from './model';

const request = (name: string, method = 'GET') => ({
  kind: 'request' as const,
  name,
  data: { method } as never,
});
const bundle: ImportBundle = {
  format: 'postman',
  collections: [
    {
      name: 'Shop',
      description: '',
      settings: {} as never,
      scripts: { preRequest: '', afterResponse: '' },
      variables: {},
      variableSecrets: [],
      environments: [{ name: 'dev', color: null, data: {}, secrets: [] }],
      items: [
        {
          kind: 'folder',
          name: 'Users',
          data: {} as never,
          children: [request('List'), request('Create', 'POST')],
        },
        request('Health'),
      ],
    },
  ],
  environments: [{ name: 'Globals', color: null, data: {}, secrets: [] }],
  cookies: [],
  history: [],
  report: {
    format: 'postman',
    entries: [
      {
        kind: 'unsupported',
        code: 'auth_unsupported',
        path: 'Shop / X',
        message: 'digest',
        raw: { type: 'digest' },
      },
    ],
  },
};

describe('import model', () => {
  it('summarizes', () => {
    expect(summarize(bundle)).toEqual({
      collections: 1,
      folders: 1,
      requests: 3,
      environments: 2,
      cookies: 0,
    });
  });

  it('flattens the preview tree', () => {
    expect(previewRows(bundle).rows).toEqual([
      { depth: 0, kind: 'collection', name: 'Shop' },
      { depth: 1, kind: 'folder', name: 'Users' },
      { depth: 2, kind: 'request', name: 'List', method: 'GET', protocol: 'http' },
      { depth: 2, kind: 'request', name: 'Create', method: 'POST', protocol: 'http' },
      { depth: 1, kind: 'request', name: 'Health', method: 'GET', protocol: 'http' },
    ]);
    expect(previewRows(bundle, 2)).toMatchObject({ truncated: true });
  });

  it('formats the report', () => {
    expect(reportText(bundle.report, 'Report')).toBe(
      'Report\n\n[UNSUPPORTED] Shop / X: digest\n    {"type":"digest"}\n',
    );
  });
});
