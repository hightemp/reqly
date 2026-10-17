import { describe, expect, it, vi } from 'vitest';
import { FakeTransport } from '$lib/testing/fakeTransport';
import { fileToSource, filesToSources, saveExportFiles } from './importFiles';

const file = (name: string, content: BlobPart, relative = '') => {
  const f = new File([content], name);
  if (relative) Object.defineProperty(f, 'webkitRelativePath', { value: relative });
  return f;
};

describe('import sources', () => {
  it('reads text files and base64-encodes zip archives', async () => {
    expect(await fileToSource(file('a.json', '{"a":1}'))).toEqual({
      name: 'a.json',
      content: '{"a":1}',
    });
    const zip = new Uint8Array([0x50, 0x4b, 0x03, 0x04, 1, 2]);
    expect(await fileToSource(file('c.zip', zip))).toEqual({
      name: 'c.zip',
      content: 'UEsDBAEC',
      base64: true,
    });
  });

  it('keeps relative paths of directory picks and skips unrelated files', async () => {
    const sources = await filesToSources([
      file('bruno.json', '{}', 'Col/bruno.json'),
      file('a.bru', 'meta {}', 'Col/a.bru'),
      file('logo.png', 'x', 'Col/logo.png'),
      file('x.bru', '', 'Col/node_modules/x.bru'),
    ]);
    expect(sources.map((s) => s.name)).toEqual(['Col/bruno.json', 'Col/a.bru']);
  });
});

describe('saveExportFiles', () => {
  const files = [
    { name: 'a.json', mime: 'application/json', content: '{}' },
    { name: 'b.json', mime: 'application/json', content: '[]' },
  ];

  it('desktop: one file goes through the save dialog', async () => {
    const t = new FakeTransport();
    const write = vi.fn(() => ({}));
    t.handle('file.writeText', write);
    const ok = await saveExportFiles(t, files.slice(0, 1), {
      save: async () => '/tmp/x.json',
      openDirectory: async () => null,
    });
    expect(ok).toBe(true);
    expect(write).toHaveBeenCalledWith({ path: '/tmp/x.json', content: '{}' });
  });

  it('desktop: several files go into a chosen folder', async () => {
    const t = new FakeTransport();
    const write = vi.fn(() => ({}));
    t.handle('file.writeText', write);
    await saveExportFiles(t, files, { save: async () => null, openDirectory: async () => '/out' });
    expect(write.mock.calls.map((c) => (c as unknown as [{ path: string }])[0].path)).toEqual([
      '/out/a.json',
      '/out/b.json',
    ]);
  });

  it('desktop: cancelling saves nothing', async () => {
    const t = new FakeTransport();
    expect(
      await saveExportFiles(t, files.slice(0, 1), {
        save: async () => null,
        openDirectory: async () => null,
      }),
    ).toBe(false);
  });

  it('browser: downloads each file', async () => {
    const download = vi.fn();
    await saveExportFiles(new FakeTransport(), files, { download });
    expect(download.mock.calls.map((c) => c[1])).toEqual(['a.json', 'b.json']);
  });
});
