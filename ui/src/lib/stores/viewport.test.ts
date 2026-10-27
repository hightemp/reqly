import { describe, expect, it } from 'vitest';
import { Viewport } from './viewport.svelte';

function fakeMedia(matches: boolean) {
  const listeners = new Set<() => void>();
  return {
    matches,
    addEventListener: (_: 'change', l: () => void) => listeners.add(l),
    removeEventListener: (_: 'change', l: () => void) => listeners.delete(l),
    set(value: boolean) {
      this.matches = value;
      listeners.forEach((l) => l());
    },
    listeners,
  };
}

describe('Viewport', () => {
  it('is wide and mouse-driven without media queries', () => {
    const v = new Viewport({});
    expect(v.compact).toBe(false);
    expect(v.touch).toBe(false);
  });

  it('follows the compact and touch media queries', () => {
    const compact = fakeMedia(true);
    const touch = fakeMedia(false);
    const v = new Viewport({ compact, touch });
    expect(v.compact).toBe(true);
    expect(v.touch).toBe(false);
    compact.set(false);
    touch.set(true);
    expect(v.compact).toBe(false);
    expect(v.touch).toBe(true);
  });

  it('stops listening when destroyed', () => {
    const compact = fakeMedia(false);
    const v = new Viewport({ compact });
    v.destroy();
    expect(compact.listeners.size).toBe(0);
  });
});
