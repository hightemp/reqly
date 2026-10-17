import { describe, expect, it } from 'vitest';
import { charsetOf, decodeBody, detectView, parseSetCookie } from './body';

const bytes = (s: string) => new TextEncoder().encode(s);

describe('detectView', () => {
  it('uses the content type', () => {
    expect(detectView('application/json; charset=utf-8', bytes('{}'))).toBe('json');
    expect(detectView('application/problem+json', bytes('{}'))).toBe('json');
    expect(detectView('text/xml', bytes('<a/>'))).toBe('xml');
    expect(detectView('application/atom+xml', bytes('<a/>'))).toBe('xml');
    expect(detectView('text/html', bytes('<p>'))).toBe('html');
    expect(detectView('image/png', new Uint8Array([137, 80]))).toBe('image');
    expect(detectView('image/svg+xml', bytes('<svg/>'))).toBe('image');
    expect(detectView('application/pdf', bytes('%PDF'))).toBe('pdf');
    expect(detectView('audio/mpeg', new Uint8Array([1]))).toBe('audio');
    expect(detectView('video/mp4', new Uint8Array([1]))).toBe('video');
    expect(detectView('text/plain', bytes('hi'))).toBe('text');
  });

  it('sniffs when the content type is missing or generic', () => {
    expect(detectView(null, bytes('  {"a":1}'))).toBe('json');
    expect(detectView('application/octet-stream', bytes('[1,2]'))).toBe('json');
    expect(detectView(null, bytes('<?xml version="1.0"?><a/>'))).toBe('xml');
    expect(detectView(null, bytes('<!DOCTYPE html><html>'))).toBe('html');
    expect(detectView(null, new Uint8Array([0x89, 0x50, 0x4e, 0x47]))).toBe('image');
    expect(detectView(null, bytes('%PDF-1.7'))).toBe('pdf');
    expect(detectView(null, new Uint8Array([0, 1, 2, 3, 255]))).toBe('binary');
    expect(detectView(null, bytes('plain words'))).toBe('text');
  });
});

describe('decoding', () => {
  it('reads the charset parameter', () => {
    expect(charsetOf('text/html; charset="windows-1251"')).toBe('windows-1251');
    expect(charsetOf('application/json')).toBe('utf-8');
  });

  it('decodes with the given charset and falls back to utf-8', () => {
    expect(decodeBody(new Uint8Array([0xcf, 0xf0, 0xe8]), 'windows-1251')).toBe('При');
    expect(decodeBody(bytes('ok'), 'no-such-charset')).toBe('ok');
  });
});

describe('parseSetCookie', () => {
  it('parses attributes', () => {
    expect(
      parseSetCookie(
        'sid=abc=1; Path=/; Domain=.example.com; Max-Age=60; Secure; HttpOnly; SameSite=Lax',
      ),
    ).toEqual({
      name: 'sid',
      value: 'abc=1',
      path: '/',
      domain: '.example.com',
      expires: null,
      maxAge: '60',
      secure: true,
      httpOnly: true,
      sameSite: 'Lax',
    });
  });
});
