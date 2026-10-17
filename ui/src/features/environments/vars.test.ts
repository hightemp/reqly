import { describe, expect, it } from 'vitest';
import { fromRows, isPlainName, parseJsonData, toRows } from './vars';

let n = 0;
const id = () => `r${n++}`;

describe('environment rows', () => {
  it('round-trips strings, JSON values and secrets', () => {
    const data = { host: 'x', port: 80, flags: { a: true }, token: 's' };
    const rows = toRows(data, ['token'], id);
    expect(rows.map((r) => [r.name, r.value, r.json, r.secret])).toEqual([
      ['host', 'x', false, false],
      ['port', '80', true, false],
      ['flags', '{"a":true}', true, false],
      ['token', 's', false, true],
    ]);
    expect(fromRows(rows)).toEqual({ data, secrets: ['token'] });
  });

  it('keeps invalid JSON edits as strings and skips empty names', () => {
    const rows = toRows({ port: 80 }, [], id);
    rows[0]!.value = '80x';
    rows.push({ id: id(), name: ' ', value: 'v', json: false, secret: false });
    expect(fromRows(rows).data).toEqual({ port: '80x' });
  });

  it('later duplicates win', () => {
    const rows = [
      { id: id(), name: 'a', value: '1', json: false, secret: true },
      { id: id(), name: 'a', value: '2', json: false, secret: false },
    ];
    expect(fromRows(rows)).toEqual({ data: { a: '2' }, secrets: [] });
  });
});

describe('parseJsonData', () => {
  it('accepts objects only', () => {
    expect(parseJsonData('{"a":1}')).toEqual({ ok: true, data: { a: 1 } });
    expect(parseJsonData('')).toEqual({ ok: true, data: {} });
    expect(parseJsonData('[1]').ok).toBe(false);
    expect(parseJsonData('{').ok).toBe(false);
  });
});

it('isPlainName', () => {
  expect(isPlainName('base_url')).toBe(true);
  expect(isPlainName('токен')).toBe(true);
  expect(isPlainName('a b')).toBe(false);
});
