// Transport between the UI and the Rust core.
//
// Desktop calls the core through Tauri `invoke`; the browser build calls the
// server over HTTP and receives events over a WebSocket. Both expose the same
// typed `call` / `subscribe` API, so features never know which one they use.

import type { CommandName, Commands } from './generated/commands';
import type { CoreError } from './generated/CoreError';
import type { Event } from './generated/Event';

export type ReqlyEvent = Event;
export type EventListener = (event: ReqlyEvent) => void;
export type Input<N extends CommandName> = Commands[N]['input'];
export type Output<N extends CommandName> = Commands[N]['output'];

export class ApiError extends Error {
  readonly code: string;
  readonly params: Record<string, unknown>;

  constructor(error: CoreError) {
    super(error.message);
    this.name = 'ApiError';
    this.code = error.code;
    this.params = error.params ?? {};
  }

  static from(value: unknown): ApiError {
    if (value instanceof ApiError) return value;
    if (value && typeof value === 'object' && 'code' in value) {
      const v = value as Partial<CoreError>;
      return new ApiError({
        code: String(v.code),
        message: String(v.message ?? ''),
        params: v.params ?? {},
      });
    }
    const message = value instanceof Error ? value.message : String(value);
    return new ApiError({ code: 'transport.unknown', message, params: {} });
  }
}

export interface Transport {
  readonly kind: 'tauri' | 'web';
  call<N extends CommandName>(name: N, input: Input<N>): Promise<Output<N>>;
  /** Subscribes to every core event; returns an unsubscribe function. */
  subscribe(listener: EventListener): () => void;
}

/** Synthetic topics emitted by transports themselves. */
export const TRANSPORT_TOPICS = {
  /** The event stream was interrupted; state may be stale and should be reloaded. */
  reconnected: 'transport.reconnected',
  disconnected: 'transport.disconnected',
  /** The server session expired or was revoked. */
  unauthorized: 'transport.unauthorized',
  lagged: 'transport.lagged',
} as const;

// ---------------------------------------------------------------------------

export interface WebTransportOptions {
  baseUrl: string;
  fetch?: typeof fetch;
  WebSocket?: typeof WebSocket;
  reconnectDelaysMs?: number[];
}

export class WebTransport implements Transport {
  readonly kind = 'web';
  private readonly baseUrl: string;
  private readonly fetchImpl: typeof fetch;
  private readonly SocketImpl: typeof WebSocket;
  private readonly delays: number[];
  private readonly listeners = new Set<EventListener>();
  private socket: WebSocket | null = null;
  private attempt = 0;
  private everConnected = false;
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;

  constructor(options: WebTransportOptions) {
    this.baseUrl = options.baseUrl.replace(/\/$/, '');
    this.fetchImpl = options.fetch ?? fetch.bind(globalThis);
    this.SocketImpl = options.WebSocket ?? WebSocket;
    this.delays = options.reconnectDelaysMs ?? [500, 1000, 2000, 5000, 10000];
  }

  async call<N extends CommandName>(name: N, input: Input<N>): Promise<Output<N>> {
    let response: Response;
    try {
      response = await this.fetchImpl(`${this.baseUrl}/api/v1/cmd/${name}`, {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify(input),
        credentials: 'same-origin',
      });
    } catch (e) {
      throw new ApiError({ code: 'transport.network', message: String(e), params: { name } });
    }
    const body: unknown = await response.json().catch(() => null);
    if (response.status === 401) this.emit({ topic: TRANSPORT_TOPICS.unauthorized, payload: null });
    if (!response.ok) {
      throw body
        ? ApiError.from(body)
        : new ApiError({
            code: 'transport.http',
            message: response.statusText,
            params: { status: response.status },
          });
    }
    return body as Output<N>;
  }

  subscribe(listener: EventListener): () => void {
    this.listeners.add(listener);
    if (!this.socket && !this.reconnectTimer) this.connect();
    return () => {
      this.listeners.delete(listener);
      if (this.listeners.size === 0) this.disconnect();
    };
  }

  private connect() {
    const url = `${this.baseUrl.replace(/^http/, 'ws')}/api/v1/ws`;
    const socket = new this.SocketImpl(url);
    this.socket = socket;

    socket.onopen = () => {
      if (this.everConnected) this.emit({ topic: TRANSPORT_TOPICS.reconnected, payload: null });
      this.everConnected = true;
      this.attempt = 0;
    };
    socket.onmessage = (e: MessageEvent<string>) => this.handleMessage(e.data);
    socket.onclose = () => {
      if (this.socket !== socket) return;
      this.socket = null;
      if (this.everConnected && this.attempt === 0) {
        this.emit({ topic: TRANSPORT_TOPICS.disconnected, payload: null });
      }
      if (this.listeners.size > 0) this.scheduleReconnect();
    };
  }

  private handleMessage(raw: string) {
    let msg: { type?: string; event?: ReqlyEvent; skipped?: number };
    try {
      msg = JSON.parse(raw);
    } catch {
      return;
    }
    if (msg.type === 'event' && msg.event) this.emit(msg.event);
    else if (msg.type === 'lagged') {
      this.emit({ topic: TRANSPORT_TOPICS.lagged, payload: { skipped: msg.skipped } });
    }
  }

  private scheduleReconnect() {
    const delay = this.delays[Math.min(this.attempt, this.delays.length - 1)] ?? 1000;
    this.attempt += 1;
    this.reconnectTimer = setTimeout(() => {
      this.reconnectTimer = null;
      if (this.listeners.size > 0) this.connect();
    }, delay);
  }

  private disconnect() {
    if (this.reconnectTimer) clearTimeout(this.reconnectTimer);
    this.reconnectTimer = null;
    const socket = this.socket;
    this.socket = null;
    socket?.close();
  }

  private emit(event: ReqlyEvent) {
    for (const listener of this.listeners) listener(event);
  }
}

// ---------------------------------------------------------------------------

type Invoke = (cmd: string, args: Record<string, unknown>) => Promise<unknown>;
type Listen = (event: string, handler: (e: { payload: ReqlyEvent }) => void) => Promise<() => void>;

export const TAURI_EVENT = 'reqly://event';

export class TauriTransport implements Transport {
  readonly kind = 'tauri';
  private readonly listeners = new Set<EventListener>();
  private unlisten: Promise<() => void> | null = null;

  constructor(private readonly api: { invoke: Invoke; listen: Listen }) {}

  async call<N extends CommandName>(name: N, input: Input<N>): Promise<Output<N>> {
    try {
      return (await this.api.invoke('dispatch', { name, input })) as Output<N>;
    } catch (e) {
      throw ApiError.from(e);
    }
  }

  subscribe(listener: EventListener): () => void {
    this.listeners.add(listener);
    this.unlisten ??= this.api.listen(TAURI_EVENT, (e) => {
      for (const l of this.listeners) l(e.payload);
    });
    return () => {
      this.listeners.delete(listener);
      if (this.listeners.size === 0 && this.unlisten) {
        const pending = this.unlisten;
        this.unlisten = null;
        void pending.then((off) => off());
      }
    };
  }
}

// ---------------------------------------------------------------------------

export function isTauri(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

/** Android / iOS shells have no window frame to draw. */
export function isMobile(): boolean {
  return typeof navigator !== 'undefined' && /Android|iPhone|iPad/.test(navigator.userAgent);
}

/** Picks the transport for the current runtime. */
export async function createTransport(): Promise<Transport> {
  if (isTauri()) {
    const [{ invoke }, { listen }] = await Promise.all([
      import('@tauri-apps/api/core'),
      import('@tauri-apps/api/event'),
    ]);
    return new TauriTransport({ invoke, listen: listen as unknown as Listen });
  }
  return new WebTransport({ baseUrl: window.location.origin });
}
