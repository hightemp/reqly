// Web globals faker-js uses that QuickJS lacks (`image.dataUri`, …).
(() => {
  const B64 = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/';

  globalThis.TextEncoder ??= class TextEncoder {
    get encoding() {
      return 'utf-8';
    }
    encode(input = '') {
      const bytes = [];
      for (const ch of String(input)) {
        let c = ch.codePointAt(0);
        if (c >= 0xd800 && c <= 0xdfff) c = 0xfffd;
        if (c < 0x80) bytes.push(c);
        else if (c < 0x800) bytes.push(0xc0 | (c >> 6), 0x80 | (c & 63));
        else if (c < 0x10000) bytes.push(0xe0 | (c >> 12), 0x80 | ((c >> 6) & 63), 0x80 | (c & 63));
        else
          bytes.push(0xf0 | (c >> 18), 0x80 | ((c >> 12) & 63), 0x80 | ((c >> 6) & 63), 0x80 | (c & 63));
      }
      return new Uint8Array(bytes);
    }
  };

  globalThis.btoa ??= (input) => {
    const s = String(input);
    let out = '';
    for (let i = 0; i < s.length; i += 3) {
      const a = s.charCodeAt(i),
        b = s.charCodeAt(i + 1),
        c = s.charCodeAt(i + 2);
      if (a > 255 || b > 255 || c > 255) throw new Error('btoa: the string contains characters outside Latin1');
      const n = (a << 16) | ((b || 0) << 8) | (c || 0);
      out +=
        B64[(n >> 18) & 63] +
        B64[(n >> 12) & 63] +
        (isNaN(b) ? '=' : B64[(n >> 6) & 63]) +
        (isNaN(c) ? '=' : B64[n & 63]);
    }
    return out;
  };
})();
