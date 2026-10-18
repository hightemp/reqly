import { describe, expect, it } from 'vitest';
import { completionContext, parseTag, scanTemplates, serializeTag } from './scan';

describe('scanTemplates', () => {
  it('finds variables, calls and tags with ranges', () => {
    const src = 'a{{ base }}/{{ _.api.v }}?x={% uuid %}&y={{ $faker.person.firstName }}';
    const found = scanTemplates(src);
    expect(found.map((t) => [t.kind, t.name, src.slice(t.from, t.to)])).toEqual([
      ['variable', 'base', '{{ base }}'],
      ['variable', 'api.v', '{{ _.api.v }}'],
      ['tag', 'uuid', '{% uuid %}'],
      ['function', 'faker', '{{ $faker.person.firstName }}'],
    ]);
  });

  it('skips escapes, comments and unterminated tags', () => {
    expect(scanTemplates('\\{{ no }} {# c #} {{ open')).toEqual([]);
  });

  it('skips braces inside object, array and string arguments', () => {
    const src =
      "{{ $faker.helpers.objectValue({ a: { b: '}}' } }) }} {{ $faker.helpers.arrayElement(['x']) }}!";
    const found = scanTemplates(src);
    expect(found.map((t) => src.slice(t.from, t.to))).toEqual([
      "{{ $faker.helpers.objectValue({ a: { b: '}}' } }) }}",
      "{{ $faker.helpers.arrayElement(['x']) }}",
    ]);
  });

  it('keeps the root name of indexed paths', () => {
    expect(scanTemplates('{{ items[0].id }}')[0]!.root).toBe('items');
    expect(scanTemplates("{{ _['odd key'] }}")[0]!.root).toBe('odd key');
  });
});

describe('parseTag / serializeTag', () => {
  it('round-trips arguments', () => {
    const tag = parseTag("{% hash 'sha256', 'hex', token, 1.5, true %}")!;
    expect(tag).toEqual({
      name: 'hash',
      args: [
        { kind: 'string', value: 'sha256' },
        { kind: 'string', value: 'hex' },
        { kind: 'variable', value: 'token' },
        { kind: 'number', value: '1.5' },
        { kind: 'boolean', value: 'true' },
      ],
    });
    expect(serializeTag(tag)).toBe("{% hash 'sha256', 'hex', token, 1.5, true %}");
  });

  it('escapes quotes', () => {
    const s = serializeTag({ name: 'base64', args: [{ kind: 'string', value: "it's" }] });
    expect(s).toBe("{% base64 'it\\'s' %}");
    expect(parseTag(s)!.args[0]!.value).toBe("it's");
  });

  it('leaves object and array arguments to text editing', () => {
    expect(parseTag("{% faker 'number.int', { min: 1, max: 2 } %}")).toBeNull();
    expect(parseTag("{{ $faker.helpers.arrayElement(['a', 'b']) }}")).toBeNull();
  });

  it('parses dollar calls', () => {
    expect(parseTag("{{ $random('int', 1, 5) }}")).toEqual({
      name: 'random',
      args: [
        { kind: 'string', value: 'int' },
        { kind: 'number', value: '1' },
        { kind: 'number', value: '5' },
      ],
    });
  });
});

describe('completionContext', () => {
  it('detects variable, tag and faker positions', () => {
    expect(completionContext('x {{ ba')).toEqual({ kind: 'variable', prefix: 'ba', from: 5 });
    expect(completionContext('{{')).toEqual({ kind: 'variable', prefix: '', from: 2 });
    expect(completionContext('{% ha')).toEqual({ kind: 'tag', prefix: 'ha', from: 3 });
    expect(completionContext('{{ $faker.person.fi')).toEqual({
      kind: 'faker',
      prefix: 'person.fi',
      from: 10,
    });
    expect(completionContext('{{ $ra')).toEqual({ kind: 'function', prefix: 'ra', from: 4 });
    expect(completionContext('{{ a }} b')).toBeNull();
    expect(completionContext('plain')).toBeNull();
  });
});
