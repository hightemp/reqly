export interface ToastOptions {
  message: string;
  kind?: 'info' | 'success' | 'warning' | 'error';
  action?: { label: string; run: () => void };
  /** 0 keeps the toast until dismissed. */
  timeoutMs?: number;
}

export interface Toast extends ToastOptions {
  id: number;
}

class ToastService {
  items = $state<Toast[]>([]);
  private nextId = 1;
  private timers = new Map<number, ReturnType<typeof setTimeout>>();

  show(options: ToastOptions): number {
    const id = this.nextId++;
    const toast: Toast = { kind: 'info', timeoutMs: 4000, ...options, id };
    // Keep at most 4 on screen.
    this.items = [...this.items.slice(-3), toast];
    if (toast.timeoutMs)
      this.timers.set(
        id,
        setTimeout(() => this.dismiss(id), toast.timeoutMs),
      );
    return id;
  }

  error(message: string) {
    return this.show({ message, kind: 'error', timeoutMs: 8000 });
  }

  dismiss(id: number) {
    clearTimeout(this.timers.get(id));
    this.timers.delete(id);
    this.items = this.items.filter((t) => t.id !== id);
  }

  clear() {
    for (const id of this.timers.keys()) clearTimeout(this.timers.get(id));
    this.timers.clear();
    this.items = [];
  }
}

export const toasts = new ToastService();
