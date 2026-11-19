import { describe, expect, it } from 'vitest';
import { highlightToHtml } from './highlight';

describe('highlightToHtml', () => {
  it('marks JSON tokens with classes and escapes text', () => {
    const html = highlightToHtml('{"a": "<b>", "n": 1, "t": true}', 'json')!;
    expect(html).toContain('<span class="hl-property">&quot;a&quot;</span>');
    expect(html).toContain('<span class="hl-string">&quot;&lt;b&gt;&quot;</span>');
    expect(html).toContain('<span class="hl-number">1</span>');
    expect(html).toContain('<span class="hl-boolean">true</span>');
    expect(html).not.toContain('<b>');
  });

  it('knows JavaScript under its usual names', () => {
    for (const lang of ['js', 'javascript', 'ts', 'typescript']) {
      expect(highlightToHtml('const x = 1; // c', lang)).toContain('hl-keyword');
    }
  });

  it('keeps the text unchanged apart from markup', () => {
    const code = 'a: 1\nb:\n  - "x"';
    const html = highlightToHtml(code, 'yaml')!;
    const text = html.replace(/<[^>]+>/g, '').replace(/&quot;/g, '"');
    expect(text).toBe(code);
  });

  it('returns null for languages it does not know', () => {
    expect(highlightToHtml('GET / HTTP/1.1', 'http')).toBeNull();
  });
});
