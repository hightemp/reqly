import { describe, expect, it } from 'vitest';
import { isRequestText, pastedRequest } from './requestText';

describe('pastedRequest', () => {
  it('recognizes cURL commands and HTTP request text', () => {
    expect(pastedRequest("curl -X POST 'https://x.test'")).toBe('curl');
    expect(pastedRequest('  curl.exe https://x.test')).toBe('curl');
    expect(pastedRequest('GET /a HTTP/1.1\nHost: x.test')).toBe('http');
    expect(pastedRequest('POST https://x.test/a\nContent-Type: application/json\n\n{}')).toBe(
      'http',
    );
    expect(pastedRequest('### List\nGET https://x.test/a')).toBe('http');
    expect(pastedRequest('GET https://x.test/a HTTP/2')).toBe('http');
  });

  it('leaves plain URLs and text alone', () => {
    expect(pastedRequest('https://x.test/a?b=1')).toBeNull();
    expect(pastedRequest('GET https://x.test/a')).toBeNull();
    expect(pastedRequest('get the milk\nand bread')).toBeNull();
    expect(pastedRequest('{{host}}/users')).toBeNull();
  });
});

describe('isRequestText', () => {
  it('also takes a bare request line', () => {
    expect(isRequestText('GET https://x.test/a')).toBe(true);
    expect(isRequestText('curl https://x.test')).toBe(true);
    expect(isRequestText('https://x.test/a')).toBe(false);
    expect(isRequestText('hello')).toBe(false);
  });
});
