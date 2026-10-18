import { describe, expect, it, vi } from 'vitest';
import * as Y from 'yjs';
import { FakeTransport } from '$lib/testing/fakeTransport';
import { CollabService, fromBase64, toBase64, userColor } from './collab';

/** One server document relayed between clients, like the real server. */
function server() {
  const doc = new Y.Doc();
  doc.getText('t').insert(0, 'hello');
  const clients: FakeTransport[] = [];
  let docId = 'd1';
  const connect = () => {
    const t = new FakeTransport();
    clients.push(t);
    t.handle('collab.join', () => ({
      docId,
      update: toBase64(Y.encodeStateAsUpdate(doc)),
    }));
    t.handle('collab.update', (input) => {
      if (input.docId !== docId) throw { code: 'collab.stale', message: 'stale', params: {} };
      Y.applyUpdate(doc, fromBase64(input.update));
      for (const c of clients) c.emit({ topic: 'collab.update', payload: input });
      return {};
    });
    t.handle('collab.awareness', (input) => {
      for (const c of clients) c.emit({ topic: 'collab.awareness', payload: input });
      return {};
    });
    return t;
  };
  return { doc, connect, restart: () => (docId = 'd2') };
}

const user = (name: string) => () => ({ name, color: userColor(name) });

describe('CollabService', () => {
  it('merges edits of two clients and shares cursors', async () => {
    const s = server();
    const a = new CollabService(s.connect(), user('ann'));
    const b = new CollabService(s.connect(), user('bob'));
    const ba = await a.bind('n1', 'body.text', () => {});
    const bb = await b.bind('n1', 'body.text', () => {});
    expect(a.paths('n1')).toEqual(['body.text']);

    ba.ytext.insert(5, ' world');
    bb.ytext.insert(0, '> ');
    await vi.waitFor(() => {
      expect(ba.ytext.toString()).toBe('> hello world');
      expect(bb.ytext.toString()).toBe('> hello world');
    });
    expect(s.doc.getText('t').toString()).toBe('> hello world');
    await vi.waitFor(() =>
      expect([...bb.awareness.getStates().values()].map((st) => st.user?.name)).toContain('ann'),
    );

    ba.destroy();
    expect(a.paths('n1')).toEqual([]);
  });

  it('asks to bind again when the server forgot the document', async () => {
    const s = server();
    const a = new CollabService(s.connect(), user('ann'));
    const onStale = vi.fn();
    const binding = await a.bind('n1', 'url', onStale);
    s.restart();
    binding.ytext.insert(0, 'x');
    await vi.waitFor(() => expect(onStale).toHaveBeenCalledOnce());
    expect(a.paths('n1')).toEqual([]);
  });

  it('is off without a signed-in user', () => {
    expect(new CollabService(new FakeTransport(), () => null).enabled).toBe(false);
  });
});
