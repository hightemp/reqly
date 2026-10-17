// Files for import (text, or base64 for archives) and saving exports.

import type { ExportFile } from '$lib/api/generated/ExportFile';
import type { ImportSource } from '$lib/api/generated/ImportSource';
import type { Transport } from '$lib/api/transport';
import { isTauri } from '$lib/api/transport';
import { toBase64 } from './files';

/** Larger single files are rejected before reading. */
export const MAX_IMPORT_FILE_BYTES = 64 * 1024 * 1024;
/** Directory imports only read the files importers understand. */
const DIRECTORY_FILES = /\.(bru|json|ya?ml)$/i;

const isZip = (bytes: Uint8Array) =>
  bytes.length > 3 &&
  bytes[0] === 0x50 &&
  bytes[1] === 0x4b &&
  bytes[2] === 0x03 &&
  bytes[3] === 0x04;

/** Reads a file as an import source; archives travel as base64. */
export async function fileToSource(file: File, path = file.name): Promise<ImportSource> {
  if (file.size > MAX_IMPORT_FILE_BYTES) throw new Error(`${file.name}: file is too large`);
  const bytes = new Uint8Array(await file.arrayBuffer());
  if (isZip(bytes)) return { name: path, content: toBase64(bytes), base64: true };
  return { name: path, content: new TextDecoder().decode(bytes) };
}

/** Sources from a file list; directory picks keep their relative paths. */
export async function filesToSources(files: Iterable<File>): Promise<ImportSource[]> {
  const list = [...files];
  const fromDirectory = list.some((f) => f.webkitRelativePath);
  const picked = fromDirectory
    ? list.filter(
        (f) => DIRECTORY_FILES.test(f.name) && !/(^|\/)node_modules\//.test(f.webkitRelativePath),
      )
    : list;
  return Promise.all(picked.map((f) => fileToSource(f, f.webkitRelativePath || f.name)));
}

/** Opens the system chooser for files or a directory. */
export function chooseImportFiles(directory: boolean): Promise<File[]> {
  return new Promise((resolve) => {
    const input = document.createElement('input');
    input.type = 'file';
    input.multiple = true;
    if (directory) input.webkitdirectory = true;
    input.style.display = 'none';
    const done = (files: File[]) => {
      resolve(files);
      input.remove();
    };
    input.addEventListener('change', () => done([...(input.files ?? [])]));
    input.addEventListener('cancel', () => done([]));
    document.body.appendChild(input);
    input.click();
  });
}

interface FsEntry {
  isFile: boolean;
  isDirectory: boolean;
  name: string;
  fullPath: string;
  file?: (ok: (f: File) => void, err: (e: unknown) => void) => void;
  createReader?: () => {
    readEntries: (ok: (e: FsEntry[]) => void, err: (e: unknown) => void) => void;
  };
}

async function readEntry(entry: FsEntry, out: ImportSource[]): Promise<void> {
  const path = entry.fullPath.replace(/^\//, '');
  if (entry.isFile && entry.file) {
    if (!DIRECTORY_FILES.test(entry.name) && path.includes('/')) return;
    const file = await new Promise<File>((ok, err) => entry.file!(ok, err));
    out.push(await fileToSource(file, path));
    return;
  }
  if (entry.isDirectory && entry.createReader && entry.name !== 'node_modules') {
    const reader = entry.createReader();
    // readEntries returns batches until empty.
    for (;;) {
      const batch = await new Promise<FsEntry[]>((ok, err) => reader.readEntries(ok, err));
      if (batch.length === 0) break;
      for (const child of batch) await readEntry(child, out);
    }
  }
}

/** Sources from a drop, including dropped folders. */
export async function dropToSources(data: DataTransfer): Promise<ImportSource[]> {
  const entries = [...data.items]
    .map((item) => item.webkitGetAsEntry?.() as unknown as FsEntry | null | undefined)
    .filter((e): e is FsEntry => !!e);
  if (entries.length === 0) return filesToSources(data.files);
  const out: ImportSource[] = [];
  for (const entry of entries) await readEntry(entry, out);
  return out;
}

interface SaveDeps {
  save?: (options: { defaultPath?: string }) => Promise<string | null>;
  openDirectory?: () => Promise<string | null>;
  download?: (blob: Blob, name: string) => void;
}

async function tauriSaveDeps(): Promise<SaveDeps | undefined> {
  if (!isTauri()) return undefined;
  const { open, save } = await import('@tauri-apps/plugin-dialog');
  return {
    save,
    openDirectory: async () => {
      const picked = await open({ directory: true, multiple: false });
      return Array.isArray(picked) ? (picked[0] ?? null) : picked;
    },
  };
}

function download(blob: Blob, name: string) {
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = name;
  a.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

/**
 * Saves exported files. Desktop: a save dialog for one file, a folder
 * picker for several. Browser: downloads. Returns false when cancelled.
 */
export async function saveExportFiles(
  transport: Transport,
  files: ExportFile[],
  deps?: SaveDeps,
): Promise<boolean> {
  const d = deps ?? (await tauriSaveDeps());
  if (d?.save && d.openDirectory) {
    if (files.length === 1) {
      const path = await d.save({ defaultPath: files[0]!.name });
      if (!path) return false;
      await transport.call('file.writeText', { path, content: files[0]!.content });
      return true;
    }
    const dir = await d.openDirectory();
    if (!dir) return false;
    const sep = dir.includes('\\') && !dir.includes('/') ? '\\' : '/';
    for (const f of files) {
      await transport.call('file.writeText', { path: `${dir}${sep}${f.name}`, content: f.content });
    }
    return true;
  }
  for (const f of files) (d?.download ?? download)(new Blob([f.content], { type: f.mime }), f.name);
  return true;
}
