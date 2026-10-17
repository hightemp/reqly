// Helpers for showing response bodies.

export type BodyView =
  'json' | 'xml' | 'html' | 'image' | 'pdf' | 'audio' | 'video' | 'text' | 'binary';

export function mimeOf(contentType: string | null | undefined): string {
  return (contentType ?? '').split(';')[0]!.trim().toLowerCase();
}

export function charsetOf(contentType: string | null | undefined): string {
  const m = /charset\s*=\s*"?([^";]+)"?/i.exec(contentType ?? '');
  return m ? m[1]!.trim().toLowerCase() : 'utf-8';
}

function startsWith(bytes: Uint8Array, sig: number[]) {
  return sig.every((b, i) => bytes[i] === b);
}

function sniff(bytes: Uint8Array): BodyView {
  if (
    startsWith(bytes, [0x89, 0x50, 0x4e, 0x47]) ||
    startsWith(bytes, [0xff, 0xd8, 0xff]) ||
    startsWith(bytes, [0x47, 0x49, 0x46, 0x38])
  ) {
    return 'image';
  }
  if (startsWith(bytes, [0x25, 0x50, 0x44, 0x46])) return 'pdf';
  const head = new TextDecoder('utf-8', { fatal: false })
    .decode(bytes.subarray(0, 512))
    .trimStart();
  if (/^[{[]/.test(head)) return 'json';
  if (/^<!doctype html|^<html/i.test(head)) return 'html';
  if (/^<\?xml|^<[a-z_][\w:.-]*[\s>/]/i.test(head)) return 'xml';
  // Many control bytes → binary.
  const sample = bytes.subarray(0, 512);
  let control = 0;
  for (const b of sample) if (b < 9 || (b > 13 && b < 32)) control++;
  return sample.length && control / sample.length > 0.1 ? 'binary' : 'text';
}

export function detectView(contentType: string | null | undefined, bytes: Uint8Array): BodyView {
  const mime = mimeOf(contentType);
  if (mime === 'application/json' || mime.endsWith('+json') || mime === 'text/json') return 'json';
  if (mime === 'image/svg+xml' || mime.startsWith('image/')) return 'image';
  if (mime === 'application/xml' || mime === 'text/xml' || mime.endsWith('+xml')) return 'xml';
  if (mime === 'text/html' || mime === 'application/xhtml+xml') return 'html';
  if (mime === 'application/pdf') return 'pdf';
  if (mime.startsWith('audio/')) return 'audio';
  if (mime.startsWith('video/')) return 'video';
  if (mime.startsWith('text/') && mime !== 'text/plain') return 'text';
  if (mime === 'text/plain') {
    const sniffed = sniff(bytes);
    return sniffed === 'json' ? 'json' : 'text';
  }
  return sniff(bytes);
}

export function decodeBody(bytes: Uint8Array, charset: string): string {
  try {
    return new TextDecoder(charset).decode(bytes);
  } catch {
    return new TextDecoder('utf-8').decode(bytes);
  }
}

export interface SetCookie {
  name: string;
  value: string;
  path: string | null;
  domain: string | null;
  expires: string | null;
  maxAge: string | null;
  secure: boolean;
  httpOnly: boolean;
  sameSite: string | null;
}

export function parseSetCookie(header: string): SetCookie {
  const [pair = '', ...attrs] = header.split(';');
  const eq = pair.indexOf('=');
  const cookie: SetCookie = {
    name: (eq < 0 ? pair : pair.slice(0, eq)).trim(),
    value: eq < 0 ? '' : pair.slice(eq + 1).trim(),
    path: null,
    domain: null,
    expires: null,
    maxAge: null,
    secure: false,
    httpOnly: false,
    sameSite: null,
  };
  for (const attr of attrs) {
    const [k = '', ...rest] = attr.split('=');
    const key = k.trim().toLowerCase();
    const value = rest.join('=').trim();
    if (key === 'path') cookie.path = value;
    else if (key === 'domain') cookie.domain = value;
    else if (key === 'expires') cookie.expires = value;
    else if (key === 'max-age') cookie.maxAge = value;
    else if (key === 'secure') cookie.secure = true;
    else if (key === 'httponly') cookie.httpOnly = true;
    else if (key === 'samesite') cookie.sameSite = value;
  }
  return cookie;
}
