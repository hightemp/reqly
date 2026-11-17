import { describe, expect, it } from 'vitest';
import type { KeyValue } from '$lib/api/generated/KeyValue';
import { applyUrlInput, displayUrl, pathParamNames, syncPathParams } from './urlsync';

let n = 0;
const newId = () => `id${++n}`;
const kv = (name: string, value: string, enabled = true, id = name): KeyValue => ({
  id,
  enabled,
  name,
  value,
  description: '',
  kind: 'text',
});

describe('displayUrl', () => {
  it('appends enabled params', () => {
    expect(displayUrl('https://a/x', [kv('q', 'a b'), kv('off', '1', false), kv('p', '2')])).toBe(
      'https://a/x?q=a b&p=2',
    );
    expect(displayUrl('https://a/x', [])).toBe('https://a/x');
  });

  it('keeps flag params without a value', () => {
    expect(displayUrl('http://a', [kv('debug', '')])).toBe('http://a?debug');
  });
});

describe('applyUrlInput', () => {
  it('splits query into params, preserving ids and disabled rows', () => {
    const current = [kv('q', 'old', true, 'id-q'), kv('off', '1', false, 'id-off')];
    const r = applyUrlInput('https://a/x?q=new&page=2', current, newId);
    expect(r.url).toBe('https://a/x');
    expect(r.params.map((p) => [p.id, p.enabled, p.name, p.value])).toEqual([
      ['id-q', true, 'q', 'new'],
      ['id-off', false, 'off', '1'],
      [expect.stringMatching(/^id\d+$/), true, 'page', '2'],
    ]);
  });

  it('removing the query removes enabled params only', () => {
    const r = applyUrlInput('https://a/x', [kv('q', '1'), kv('off', '1', false)], newId);
    expect(r.params.map((p) => p.name)).toEqual(['off']);
  });

  it('keeps fragments and odd characters in values', () => {
    const r = applyUrlInput('http://a/x?expr=a=b&e=#frag', [], newId);
    expect(r.url).toBe('http://a/x#frag');
    expect(r.params.map((p) => [p.name, p.value])).toEqual([
      ['expr', 'a=b'],
      ['e', ''],
    ]);
  });

  it('decodes percent-encoding for display in the table', () => {
    const r = applyUrlInput('http://a?q=a%20b%26c', [], newId);
    expect(r.params[0]!.value).toBe('a b&c');
  });

  it('keeps template variables intact', () => {
    const r = applyUrlInput('{{ base }}/users?token={{token}}', [], newId);
    expect(r.url).toBe('{{ base }}/users');
    expect(r.params[0]).toMatchObject({ name: 'token', value: '{{token}}' });
  });
});

describe('path params', () => {
  it('finds :name segments', () => {
    expect(pathParamNames('https://a/users/:id/posts/:postId?x=:no')).toEqual(['id', 'postId']);
    expect(pathParamNames('http://localhost:8080/a')).toEqual([]);
  });

  it('syncs rows keeping values of existing names', () => {
    const rows = syncPathParams(
      'http://a/:id/:other',
      [kv('id', '42', true, 'keep'), kv('gone', 'x')],
      newId,
    );
    expect(rows.map((r) => [r.id === 'keep', r.name, r.value])).toEqual([
      [true, 'id', '42'],
      [false, 'other', ''],
    ]);
  });
});
