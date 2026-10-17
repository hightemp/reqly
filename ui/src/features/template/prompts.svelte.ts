// Values for `{% prompt %}` tags, asked before a send. Remembered values
// last for the session.

import type { PromptInfo } from '$lib/api/generated/PromptInfo';

export interface PromptRequest {
  prompts: PromptInfo[];
  initial: Record<string, string>;
  resolve: (values: Record<string, string> | null) => void;
}

class PromptState {
  request = $state<PromptRequest | null>(null);
  /** Remembered for this session, by prompt key. */
  readonly remembered = new Map<string, string>();

  ask(prompts: PromptInfo[]): Promise<Record<string, string> | null> {
    const initial = Object.fromEntries(
      prompts.map((p) => [p.key, this.remembered.get(p.key) ?? p.defaultValue]),
    );
    return new Promise((resolve) => {
      this.request = {
        prompts,
        initial,
        resolve: (values) => {
          this.request = null;
          resolve(values);
        },
      };
    });
  }

  remember(values: Record<string, string>) {
    for (const [k, v] of Object.entries(values)) this.remembered.set(k, v);
  }

  forget() {
    this.remembered.clear();
  }
}

export const promptState = new PromptState();
