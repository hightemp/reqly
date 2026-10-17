import { describe, expect, it } from 'vitest';
import { formatJson, formatXml } from './format';

describe('formatJson', () => {
  it('pretty prints without parsing numbers', () => {
    const big = '{"id":12345678901234567890,"a":[1,2,{"b":null}],"s":"x, y: {z}"}';
    expect(formatJson(big)).toBe(
      '{\n  "id": 12345678901234567890,\n  "a": [\n    1,\n    2,\n    {\n      "b": null\n    }\n  ],\n  "s": "x, y: {z}"\n}',
    );
  });

  it('handles escapes and empty containers', () => {
    expect(formatJson('{"q":"say \\"hi\\"","e":{},"l":[]}')).toBe(
      '{\n  "q": "say \\"hi\\"",\n  "e": {},\n  "l": []\n}',
    );
  });

  it('re-indents already formatted input', () => {
    expect(formatJson('{\n\n   "a" :   1 }', 4)).toBe('{\n    "a": 1\n}');
  });

  it('returns null for invalid json', () => {
    expect(formatJson('{"a":')).toBeNull();
    expect(formatJson('nope')).toBeNull();
  });
});

describe('formatXml', () => {
  it('indents nested elements and keeps text inline', () => {
    expect(formatXml('<a><b x="1">text</b><c/><!-- note --></a>')).toBe(
      '<a>\n  <b x="1">text</b>\n  <c/>\n  <!-- note -->\n</a>',
    );
  });

  it('keeps the declaration on its own line', () => {
    expect(formatXml('<?xml version="1.0"?><r><i>1</i></r>')).toBe(
      '<?xml version="1.0"?>\n<r>\n  <i>1</i>\n</r>',
    );
  });
});
