import { describe, expect, it } from 'vitest';
import { aiText, loadReference, pathText } from './reference';

const nodes: Record<string, object> = {
  f1: { id: 'f1', collectionId: 'c1', parentId: null, name: 'Auth', data: { kind: 'folder' } },
  r1: {
    id: 'r1',
    collectionId: 'c1',
    parentId: 'f1',
    name: 'Login',
    data: { kind: 'request', method: 'POST', url: '{{ base }}/login' },
  },
};
const call = async (_: string, input: { id: string }) => nodes[input.id]!;

describe('request references', () => {
  it('builds the path from the collection down', async () => {
    const ref = await loadReference(call as never, [{ id: 'c1', name: 'Shop API' }], 'r1');
    expect(pathText(ref)).toBe('Shop API / Auth / Login');
    expect(ref.id).toBe('r1');
  });

  it('gives an agent everything to find the request', async () => {
    const ref = await loadReference(call as never, [{ id: 'c1', name: 'Shop API' }], 'r1');
    expect(aiText(ref)).toBe(
      [
        'Reqly request "Login"',
        'Path: Shop API / Auth / Login',
        'ID: r1',
        'POST {{ base }}/login',
        'Read it with the Reqly MCP tool get_request {"id": "r1"}.',
      ].join('\n'),
    );
  });

  it('describes a folder without a method line', async () => {
    const ref = await loadReference(call as never, [], 'f1');
    expect(pathText(ref)).toBe('Auth');
    expect(aiText(ref).split('\n')).toEqual([
      'Reqly folder "Auth"',
      'Path: Auth',
      'ID: f1',
      'Read it with the Reqly MCP tool get_request {"id": "f1"}.',
    ]);
  });
});
