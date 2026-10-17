import { describe, expect, it } from 'vitest';
import { renderMarkdown } from './markdown';

describe('renderMarkdown', () => {
  it('escapes HTML', () => {
    expect(renderMarkdown('<script>alert(1)</script>')).toBe(
      '<p>&lt;script&gt;alert(1)&lt;/script&gt;</p>',
    );
  });

  it('renders headings, emphasis and code', () => {
    expect(renderMarkdown('# Title\n**bold** and *it* `x<y`')).toBe(
      '<h1>Title</h1>\n<p><strong>bold</strong> and <em>it</em> <code>x&lt;y</code></p>',
    );
  });

  it('renders lists and code blocks', () => {
    expect(renderMarkdown('- a\n- b\n\n1. one\n2. two')).toBe(
      '<ul><li>a</li><li>b</li></ul>\n<ol><li>one</li><li>two</li></ol>',
    );
    expect(renderMarkdown('```\n<b>\n  x\n```')).toBe('<pre><code>&lt;b&gt;\n  x</code></pre>');
  });

  it('only links safe protocols', () => {
    expect(renderMarkdown('[ok](https://a.b/?x=1&y=2) [bad](javascript:alert(1))')).toBe(
      '<p><a href="https://a.b/?x=1&amp;y=2" target="_blank" rel="noopener noreferrer">ok</a> bad</p>',
    );
  });

  it('joins paragraph lines and splits on blank lines', () => {
    expect(renderMarkdown('a\nb\n\nc')).toBe('<p>a b</p>\n<p>c</p>');
  });
});
