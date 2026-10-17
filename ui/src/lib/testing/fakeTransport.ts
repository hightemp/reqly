import type { CommandName } from '$lib/api/generated/commands';
import type { EventListener, Input, Output, ReqlyEvent, Transport } from '$lib/api/transport';

type Handler = (input: unknown) => unknown;

/** In-memory transport for component and store tests. */
export class FakeTransport implements Transport {
  readonly kind = 'web';
  readonly calls: { name: string; input: unknown }[] = [];
  private readonly handlers = new Map<string, Handler>();
  private readonly listeners = new Set<EventListener>();

  handle<N extends CommandName>(
    name: N,
    handler: (input: Input<N>) => Output<N> | Promise<Output<N>>,
  ) {
    this.handlers.set(name, handler as Handler);
    return this;
  }

  async call<N extends CommandName>(name: N, input: Input<N>): Promise<Output<N>> {
    this.calls.push({ name, input });
    const handler = this.handlers.get(name);
    if (!handler) throw new Error(`FakeTransport: no handler for ${name}`);
    return (await handler(input)) as Output<N>;
  }

  subscribe(listener: EventListener): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  emit(event: ReqlyEvent) {
    this.listeners.forEach((l) => l(event));
  }

  /** One workspace with one empty collection (enough to boot the app shell). */
  withEmptyWorkspace() {
    const meta = { createdAt: 0, updatedAt: 0, version: 1, deletedAt: null, sortKey: 'V' };
    this.handle('workspace.ensureDefault', () => [{ id: 'w1', name: 'My Workspace', ...meta }]);
    this.handle('workspace.list', () => [{ id: 'w1', name: 'My Workspace', ...meta }]);
    this.handle('collection.list', () => [
      { id: 'c1', workspaceId: 'w1', name: 'My Collection', settings: {} as never, ...meta },
    ]);
    this.handle('tree.list', () => []);
    return this;
  }

  /** Backs `settings.get` / `settings.set` with a map and echoes change events. */
  withSettings(initial: Record<string, unknown> = {}) {
    const store = new Map(Object.entries(initial));
    this.handle('settings.get', ({ key }) => ({ value: store.get(key) ?? null }));
    this.handle('settings.set', ({ key, value }) => {
      store.set(key, value);
      this.emit({ topic: 'settings.changed', payload: { key, value } });
      return {};
    });
    return store;
  }
}
