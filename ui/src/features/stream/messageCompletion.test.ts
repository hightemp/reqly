import { describe, expect, it } from 'vitest';
import { keysByPath, pathAt } from './messageCompletion';

describe('message completion', () => {
  it('collects keys by path', () => {
    const keys = keysByPath({ a: 1, nested: { flag: false }, list: [{ x: 1 }] });
    expect(keys.get('')).toEqual(['a', 'nested', 'list']);
    expect(keys.get('nested')).toEqual(['flag']);
    expect(keys.get('list')).toEqual(['x']);
  });

  it('finds the object path at the cursor', () => {
    const text = '{ "a": 1, "nested": { "fl';
    expect(pathAt(text, text.length)).toBe('nested');
    expect(pathAt('{ "', 3)).toBe('');
    const list = '{ "list": [ { "';
    expect(pathAt(list, list.length)).toBe('list');
    const after = '{ "nested": { "flag": true }, "';
    expect(pathAt(after, after.length)).toBe('');
  });
});
