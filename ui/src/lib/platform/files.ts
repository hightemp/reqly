// File access that differs between desktop (native dialogs, local paths)
// and the browser (uploads to the server blob store, downloads).

import type { FileRef } from '$lib/api/generated/FileRef';
import type { Transport } from '$lib/api/transport';
import { isTauri } from '$lib/api/transport';

export interface PickedFile {
  name: string;
  /** Value for multipart file fields: a path or `blob:<hash>`. */
  value: string;
  /** Value for binary bodies. */
  ref: FileRef;
}

interface TauriDialogs {
  open?: (options?: {
    multiple?: boolean;
    directory?: boolean;
  }) => Promise<string | string[] | null>;
  save?: (options?: { defaultPath?: string }) => Promise<string | null>;
}

export interface FileDeps {
  tauri?: TauriDialogs;
  /** Browser file chooser (tests inject a fake). */
  choose?: () => Promise<File | null>;
  download?: (blob: Blob, name: string) => void;
}

const basename = (path: string) => path.split(/[\\/]/).pop() ?? path;

async function tauriDialogs(): Promise<TauriDialogs | undefined> {
  if (!isTauri()) return undefined;
  const { open, save } = await import('@tauri-apps/plugin-dialog');
  return { open, save };
}

/** Opens the system file chooser via a hidden `<input type="file">`. */
export function browserChoose(): Promise<File | null> {
  return new Promise((resolve) => {
    const input = document.createElement('input');
    input.type = 'file';
    input.style.display = 'none';
    input.addEventListener('change', () => {
      resolve(input.files?.[0] ?? null);
      input.remove();
    });
    input.addEventListener('cancel', () => {
      resolve(null);
      input.remove();
    });
    document.body.appendChild(input);
    input.click();
  });
}

function toBase64(bytes: Uint8Array): string {
  let binary = '';
  const step = 0x8000;
  for (let i = 0; i < bytes.length; i += step)
    binary += String.fromCharCode(...bytes.subarray(i, i + step));
  return btoa(binary);
}

function fromBase64(b64: string): Uint8Array {
  const binary = atob(b64);
  const out = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) out[i] = binary.charCodeAt(i);
  return out;
}

export async function pickFile(
  transport: Transport,
  deps: FileDeps = {},
): Promise<PickedFile | null> {
  const tauri = deps.tauri ?? (deps.choose ? undefined : await tauriDialogs());
  if (tauri?.open) {
    const picked = await tauri.open({ multiple: false, directory: false });
    const path = Array.isArray(picked) ? picked[0] : picked;
    if (!path) return null;
    return { name: basename(path), value: path, ref: { type: 'path', path } };
  }
  const file = await (deps.choose ?? browserChoose)();
  if (!file) return null;
  const bytes = new Uint8Array(await file.arrayBuffer());
  const blob = await transport.call('blob.upload', { name: file.name, base64: toBase64(bytes) });
  return {
    name: file.name,
    value: `blob:${blob.hash}`,
    ref: { type: 'blob', hash: blob.hash, name: file.name, size: blob.size },
  };
}

/** Loads a response body (or its first `maxBytes`) in chunks. */
export async function readResponseBody(
  transport: Transport,
  responseId: string,
  { chunkSize = 1024 * 1024, maxBytes = Infinity }: { chunkSize?: number; maxBytes?: number } = {},
): Promise<Uint8Array> {
  const parts: Uint8Array[] = [];
  let offset = 0;
  let total = Infinity;
  while (offset < Math.min(total, maxBytes)) {
    const limit = Math.min(chunkSize, maxBytes - offset);
    const chunk = await transport.call('response.body', { id: responseId, offset, limit });
    total = chunk.total;
    const bytes = fromBase64(chunk.base64);
    if (bytes.length === 0) break;
    parts.push(bytes);
    offset += bytes.length;
  }
  const out = new Uint8Array(offset);
  let at = 0;
  for (const p of parts) {
    out.set(p, at);
    at += p.length;
  }
  return out;
}

function browserDownload(blob: Blob, name: string) {
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = name;
  a.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

/** Saves a response body; returns false when the user cancelled. */
export async function saveResponseBody(
  transport: Transport,
  responseId: string,
  suggestedName: string,
  deps: FileDeps = {},
): Promise<boolean> {
  const tauri = deps.tauri ?? (deps.download ? undefined : await tauriDialogs());
  if (tauri?.save) {
    const path = await tauri.save({ defaultPath: suggestedName });
    if (!path) return false;
    await transport.call('response.saveBody', { id: responseId, path });
    return true;
  }
  const bytes = await readResponseBody(transport, responseId);
  (deps.download ?? browserDownload)(new Blob([bytes.slice().buffer]), suggestedName);
  return true;
}

export { fromBase64, toBase64 };
