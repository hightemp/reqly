// Promise-based replacements for window.confirm / window.prompt.

export interface ConfirmOptions {
  title: string;
  message?: string;
  confirmLabel?: string;
  cancelLabel?: string;
  danger?: boolean;
}

export interface PromptOptions {
  title: string;
  label: string;
  value?: string;
  placeholder?: string;
  confirmLabel?: string;
  /** Returns an error message, or null when valid. */
  validate?: (value: string) => string | null;
  /** Select the text before this index (e.g. file name without extension). */
  selectUntil?: number;
}

export type DialogEntry =
  | { id: number; kind: 'confirm'; options: ConfirmOptions; resolve: (ok: boolean) => void }
  | { id: number; kind: 'prompt'; options: PromptOptions; resolve: (value: string | null) => void };

class DialogService {
  stack = $state<DialogEntry[]>([]);
  private nextId = 1;

  confirm(options: ConfirmOptions): Promise<boolean> {
    return new Promise((resolve) => {
      const id = this.nextId++;
      this.stack.push({
        id,
        kind: 'confirm',
        options,
        resolve: (ok) => this.finish(id, () => resolve(ok)),
      });
    });
  }

  prompt(options: PromptOptions): Promise<string | null> {
    return new Promise((resolve) => {
      const id = this.nextId++;
      this.stack.push({
        id,
        kind: 'prompt',
        options,
        resolve: (v) => this.finish(id, () => resolve(v)),
      });
    });
  }

  private finish(id: number, done: () => void) {
    this.stack = this.stack.filter((d) => d.id !== id);
    done();
  }

  /** Cancels every open dialog (tests, navigation). */
  closeAll() {
    for (const d of [...this.stack]) {
      if (d.kind === 'confirm') d.resolve(false);
      else d.resolve(null);
    }
  }
}

export const dialogs = new DialogService();
