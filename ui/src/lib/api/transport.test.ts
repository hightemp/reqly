import { describe, expect, it, vi } from 'vitest';
import { ApiError, TauriTransport, WebTransport, type ReqlyEvent } from './transport';

class FakeSocket {
  static instances: FakeSocket[] = [];
  onopen: (() => void) | null = null;
  onmessage: ((e: { data: string }) => void) | null = null;
  onclose: (() => void) | null = null;
  onerror: (() => void) | null = null;
  closed = false;
  constructor(public url: string) {
    FakeSocket.instances.push(this);
  }
  close() {
    this.closed = true;
  }
  open() {
    this.onopen?.();
  }
  push(msg: unknown) {
    this.onmessage?.({ data: JSON.stringify(msg) });
  }
  drop() {
    this.onclose?.();
  }
}

function jsonResponse(status: number, body: unknown) {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json' },
  });
}

function web(fetchImpl: typeof fetch) {
  FakeSocket.instances = [];
  return new WebTransport({
    baseUrl: 'http://host:1',
    fetch: fetchImpl,
    WebSocket: FakeSocket as unknown as typeof WebSocket,
    reconnectDelaysMs: [10, 20],
  });
}

describe('WebTransport', () => {
  it('posts commands as JSON and returns output', async () => {
    const fetchMock = vi.fn(async () => jsonResponse(200, { echo: 'hi' }));
    const t = web(fetchMock);
    await expect(t.call('app.ping', { message: 'hi' })).resolves.toEqual({ echo: 'hi' });
    expect(fetchMock).toHaveBeenCalledWith(
      'http://host:1/api/v1/cmd/app.ping',
      expect.objectContaining({ method: 'POST', body: '{"message":"hi"}' }),
    );
  });

  it('rejects with ApiError carrying code and params', async () => {
    const t = web(async () =>
      jsonResponse(404, { code: 'command.unknown', message: 'x', params: { name: 'n' } }),
    );
    const err = await t.call('app.info', {}).catch((e: unknown) => e);
    expect(err).toBeInstanceOf(ApiError);
    expect(err).toMatchObject({ code: 'command.unknown', params: { name: 'n' } });
  });

  it('maps network failures to transport.network', async () => {
    const t = web(async () => {
      throw new TypeError('Failed to fetch');
    });
    await expect(t.call('app.info', {})).rejects.toMatchObject({ code: 'transport.network' });
  });

  it('opens the socket lazily and delivers events', () => {
    const t = web(vi.fn());
    expect(FakeSocket.instances).toHaveLength(0);
    const seen: ReqlyEvent[] = [];
    t.subscribe((e) => seen.push(e));
    const socket = FakeSocket.instances[0]!;
    expect(socket.url).toBe('ws://host:1/api/v1/ws');
    socket.open();
    socket.push({ type: 'hello', version: '0' });
    socket.push({ type: 'event', event: { topic: 'app.pinged', payload: 'hi' } });
    expect(seen).toEqual([{ topic: 'app.pinged', payload: 'hi' }]);
  });

  it('reconnects with backoff and announces it', async () => {
    vi.useFakeTimers();
    const t = web(vi.fn());
    const seen: string[] = [];
    t.subscribe((e) => seen.push(e.topic));
    FakeSocket.instances[0]!.open();
    FakeSocket.instances[0]!.drop();
    expect(FakeSocket.instances).toHaveLength(1);
    expect(seen).toEqual(['transport.disconnected']);
    await vi.advanceTimersByTimeAsync(10);
    expect(FakeSocket.instances).toHaveLength(2);
    FakeSocket.instances[1]!.open();
    expect(seen).toContain('transport.reconnected');
    vi.useRealTimers();
  });

  it('closes the socket when the last listener unsubscribes', () => {
    const t = web(vi.fn());
    const off = t.subscribe(() => {});
    off();
    expect(FakeSocket.instances[0]!.closed).toBe(true);
  });

  it('turns lagged messages into a resync event', () => {
    const t = web(vi.fn());
    const seen: ReqlyEvent[] = [];
    t.subscribe((e) => seen.push(e));
    FakeSocket.instances[0]!.push({ type: 'lagged', skipped: 3 });
    expect(seen).toEqual([{ topic: 'transport.lagged', payload: { skipped: 3 } }]);
  });

  it('derives wss from https', () => {
    FakeSocket.instances = [];
    const t = new WebTransport({
      baseUrl: 'https://reqly.example',
      fetch: vi.fn(),
      WebSocket: FakeSocket as unknown as typeof WebSocket,
    });
    t.subscribe(() => {});
    expect(FakeSocket.instances[0]!.url).toBe('wss://reqly.example/api/v1/ws');
  });
});

describe('TauriTransport', () => {
  it('invokes the dispatch command', async () => {
    const invoke = vi.fn(async () => ({ echo: 'yo' }));
    const t = new TauriTransport({ invoke, listen: vi.fn() });
    await expect(t.call('app.ping', { message: 'yo' })).resolves.toEqual({ echo: 'yo' });
    expect(invoke).toHaveBeenCalledWith('dispatch', { name: 'app.ping', input: { message: 'yo' } });
  });

  it('wraps rejected core errors', async () => {
    const invoke = vi.fn(async () => {
      throw { code: 'not_found', message: 'gone', params: {} };
    });
    const t = new TauriTransport({ invoke, listen: vi.fn() });
    await expect(t.call('app.info', {})).rejects.toMatchObject({ code: 'not_found' });
  });

  it('forwards tauri events to subscribers and unlistens', async () => {
    let handler: ((e: { payload: ReqlyEvent }) => void) | undefined;
    const unlisten = vi.fn();
    const listen = vi.fn(async (_name: string, h: typeof handler) => {
      handler = h;
      return unlisten;
    });
    const t = new TauriTransport({ invoke: vi.fn(), listen });
    const seen: ReqlyEvent[] = [];
    const off = t.subscribe((e) => seen.push(e));
    await Promise.resolve();
    expect(listen).toHaveBeenCalledWith('reqly://event', expect.any(Function));
    handler!({ payload: { topic: 'a', payload: 1 } });
    expect(seen).toEqual([{ topic: 'a', payload: 1 }]);
    off();
    await Promise.resolve();
    expect(unlisten).toHaveBeenCalled();
  });
});
