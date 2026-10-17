import { describe, expect, it, vi } from 'vitest';
import { FakeTransport } from '$lib/testing/fakeTransport';
import { pickFile, readResponseBody, saveResponseBody } from './files';

function file(name: string, content: string) {
  return new File([content], name, { type: 'text/plain' });
}

describe('pickFile', () => {
  it('desktop returns a local path', async () => {
    const open = vi.fn(async () => '/home/me/data.json');
    const picked = await pickFile(new FakeTransport(), { tauri: { open } });
    expect(picked).toEqual({
      name: 'data.json',
      value: '/home/me/data.json',
      ref: { type: 'path', path: '/home/me/data.json' },
    });
  });

  it('desktop cancel returns null', async () => {
    expect(await pickFile(new FakeTransport(), { tauri: { open: async () => null } })).toBeNull();
  });

  it('web uploads the file to the server blob store', async () => {
    const transport = new FakeTransport();
    const upload = vi.fn(async (input: { name: string; base64: string }) => ({
      hash: 'a'.repeat(64),
      name: input.name,
      size: 5,
    }));
    transport.handle('blob.upload', upload);
    const picked = await pickFile(transport, { choose: async () => file('a.txt', 'hello') });
    expect(upload).toHaveBeenCalledWith({ name: 'a.txt', base64: 'aGVsbG8=' });
    expect(picked).toEqual({
      name: 'a.txt',
      value: `blob:${'a'.repeat(64)}`,
      ref: { type: 'blob', hash: 'a'.repeat(64), name: 'a.txt', size: 5 },
    });
  });
});

describe('response bodies', () => {
  function transportWithBody(total: number, chunk: (offset: number, limit: number) => string) {
    const t = new FakeTransport();
    t.handle('response.body', ({ offset, limit }) => ({
      base64: btoa(chunk(offset, limit)),
      offset,
      total,
    }));
    return t;
  }

  it('reads bodies in chunks', async () => {
    const text = 'abcdefghij';
    const t = transportWithBody(10, (o, l) => text.slice(o, o + l));
    const bytes = await readResponseBody(t, 'id', { chunkSize: 4 });
    expect(new TextDecoder().decode(bytes)).toBe(text);
    expect(t.calls.filter((c) => c.name === 'response.body')).toHaveLength(3);
  });

  it('stops at the limit and reports truncation', async () => {
    const t = transportWithBody(100, (o, l) => 'x'.repeat(Math.min(l, 100 - o)));
    const bytes = await readResponseBody(t, 'id', { chunkSize: 30, maxBytes: 50 });
    expect(bytes.length).toBe(50);
  });

  it('desktop save goes through the native dialog', async () => {
    const t = new FakeTransport();
    const saveBody = vi.fn(() => ({}));
    t.handle('response.saveBody', saveBody);
    const ok = await saveResponseBody(t, 'id', 'body.json', {
      tauri: { save: async () => '/tmp/body.json' },
    });
    expect(ok).toBe(true);
    expect(saveBody).toHaveBeenCalledWith({ id: 'id', path: '/tmp/body.json' });
  });

  it('web save triggers a download', async () => {
    const t = transportWithBody(3, () => 'abc');
    const download = vi.fn();
    const ok = await saveResponseBody(t, 'id', 'body.txt', { download });
    expect(ok).toBe(true);
    expect(download).toHaveBeenCalledWith(expect.any(Blob), 'body.txt');
  });
});
