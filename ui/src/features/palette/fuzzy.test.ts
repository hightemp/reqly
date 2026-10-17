import { describe, expect, it } from 'vitest';
import { fuzzyScore, rank } from './fuzzy';

describe('fuzzyScore', () => {
  it('matches subsequences case-insensitively', () => {
    expect(fuzzyScore('lu', 'List users')).not.toBeNull();
    expect(fuzzyScore('xyz', 'List users')).toBeNull();
  });

  it('prefers consecutive and word-start matches', () => {
    expect(fuzzyScore('user', 'List users')!.score).toBeGreaterThan(
      fuzzyScore('user', 'Unused service resolver')!.score,
    );
    expect(fuzzyScore('cu', 'Create user')!.score).toBeGreaterThan(
      fuzzyScore('cu', 'Cache usage')!.score - 100,
    );
    expect(fuzzyScore('set', 'Settings')!.score).toBeGreaterThan(
      fuzzyScore('set', 'Reset theme')!.score,
    );
  });

  it('returns matched positions for highlighting', () => {
    expect(fuzzyScore('lu', 'List users')!.positions).toEqual([0, 5]);
  });

  it('empty query matches everything with score 0', () => {
    expect(fuzzyScore('', 'anything')).toEqual({ score: 0, positions: [] });
  });
});

describe('rank', () => {
  it('sorts by score and keeps original order for ties', () => {
    const items = ['Delete user', 'Users', 'List users', 'Health'];
    expect(rank('users', items, (s) => s).map((r) => r.item)).toEqual(['Users', 'List users']);
    expect(rank('', items, (s) => s).map((r) => r.item)).toEqual(items);
  });
});
