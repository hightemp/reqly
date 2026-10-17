import { describe, expect, it } from 'vitest';
import { fromBulk, toBulk, type KvRow } from './bulk';

const row = (name: string, value: string, enabled = true, id = name): KvRow => ({
  id,
  enabled,
  name,
  value,
  description: '',
  kind: 'text',
});

describe('bulk edit', () => {
  it('serializes rows, disabled ones commented out', () => {
    expect(toBulk([row('Accept', '*/*'), row('X-Off', '1', false)])).toBe(
      'Accept: */*\n// X-Off: 1',
    );
  });

  it('parses lines, keeping ids of rows with the same name', () => {
    const existing = [row('Accept', 'a', true, 'id-accept')];
    const rows = fromBulk(
      'Accept: application/json\n// X-Off: 1\n\nX-Url: http://a:b\nbroken',
      existing,
      () => 'new',
    );
    expect(rows.map((r) => [r.id, r.enabled, r.name, r.value])).toEqual([
      ['id-accept', true, 'Accept', 'application/json'],
      ['new', false, 'X-Off', '1'],
      ['new', true, 'X-Url', 'http://a:b'],
      ['new', true, 'broken', ''],
    ]);
  });

  it('round-trips', () => {
    const rows = [row('A', '1'), row('B', 'x: y', false)];
    const back = fromBulk(toBulk(rows), rows, () => 'n');
    expect(back.map((r) => [r.enabled, r.name, r.value])).toEqual([
      [true, 'A', '1'],
      [false, 'B', 'x: y'],
    ]);
  });

  it('supports = separator for form fields', () => {
    expect(fromBulk('a=1\nb = 2', [], () => 'n', '=').map((r) => [r.name, r.value])).toEqual([
      ['a', '1'],
      ['b', '2'],
    ]);
    expect(toBulk([row('a', '1')], '=')).toBe('a=1');
  });
});
