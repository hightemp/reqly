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

  it('renders tables', () => {
    expect(renderMarkdown('| A | `b\\|c` |\n|---|:-:|\n| 1 | **2** |')).toBe(
      '<table><thead><tr><th>A</th><th><code>b|c</code></th></tr></thead>' +
        '<tbody><tr><td>1</td><td><strong>2</strong></td></tr></tbody></table>',
    );
  });

  it('keeps a lone pipe line as text', () => {
    expect(renderMarkdown('a | b')).toBe('<p>a | b</p>');
  });

  it('renders help links and heading ids only when asked', () => {
    expect(renderMarkdown('[Variables](help:variables#secrets)')).toBe('<p>Variables</p>');
    expect(renderMarkdown('## Response chaining\n[Vars](help:variables)', { help: true })).toBe(
      '<h2 id="response-chaining">Response chaining</h2>\n' +
        '<p><a href="#variables" data-help="variables">Vars</a></p>',
    );
    expect(renderMarkdown('## Переменные окружения', { help: true })).toBe(
      '<h2 id="переменные-окружения">Переменные окружения</h2>',
    );
  });

  it('continues list items on indented lines', () => {
    expect(renderMarkdown('- one\n  more\n- two\ntext')).toBe(
      '<ul><li>one more</li><li>two</li></ul>\n<p>text</p>',
    );
  });

  it('renders underscore emphasis but not snake_case', () => {
    expect(renderMarkdown('_merge_ or snake_case_name')).toBe(
      '<p><em>merge</em> or snake_case_name</p>',
    );
  });
});
