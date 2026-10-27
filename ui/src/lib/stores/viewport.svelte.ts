// Screen size and input kind: the compact (phone) layout and touch targets.

import type { MediaQuery } from '$lib/themes/manager.svelte';

/** Below this width the shell switches to the compact layout. */
export const COMPACT_QUERY = '(max-width: 767px)';
export const TOUCH_QUERY = '(pointer: coarse)';

export interface ViewportOptions {
  compact?: MediaQuery;
  touch?: MediaQuery;
}

export class Viewport {
  compact = $state(false);
  touch = $state(false);
  private readonly offs: (() => void)[] = [];

  constructor(options: ViewportOptions) {
    this.watch(options.compact, (v) => (this.compact = v));
    this.watch(options.touch, (v) => (this.touch = v));
  }

  /** Media queries of the current window. */
  static fromWindow(): Viewport {
    const query = (q: string) => globalThis.matchMedia?.(q) ?? undefined;
    return new Viewport({ compact: query(COMPACT_QUERY), touch: query(TOUCH_QUERY) });
  }

  private watch(media: MediaQuery | undefined, set: (v: boolean) => void) {
    if (!media) return;
    set(media.matches);
    const listener = () => set(media.matches);
    media.addEventListener('change', listener);
    this.offs.push(() => media.removeEventListener('change', listener));
  }

  destroy() {
    this.offs.splice(0).forEach((off) => off());
  }
}
