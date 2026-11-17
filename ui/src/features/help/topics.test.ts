import { describe, expect, it } from 'vitest';
import { slug } from '$lib/markdown';
import { TOPIC_IDS, searchTopics, topic, topics } from './topics';

const LOCALES = ['en', 'ru'];

describe('help topics', () => {
  it('has every topic in every language', () => {
    for (const locale of LOCALES) {
      const list = topics(locale);
      expect(list.map((t) => t.id)).toEqual([...TOPIC_IDS]);
      for (const t of list) {
        expect(t.locale, `${locale}/${t.id}`).toBe(locale);
        expect(t.title, `${locale}/${t.id}`).toBeTruthy();
        expect(t.body.trim(), `${locale}/${t.id}`).toBeTruthy();
      }
    }
  });

  it('links only to topics and headings that exist', () => {
    for (const locale of LOCALES) {
      for (const t of topics(locale)) {
        for (const [, target, anchor] of t.body.matchAll(/\(help:([\w-]+)(?:#([^)]+))?\)/g)) {
          const linked = topic(locale, target!);
          expect(linked, `${locale}/${t.id} → ${target}`).toBeDefined();
          if (anchor) {
            const anchors = [...linked!.body.matchAll(/^#{2,6}\s+(.*)$/gm)].map((m) => slug(m[1]!));
            expect(anchors, `${locale}/${t.id} → ${target}#${anchor}`).toContain(anchor);
          }
        }
      }
    }
  });

  it('falls back to English for an unknown language', () => {
    expect(topics('de')[0]!.locale).toBe('en');
  });

  it('searches titles first, then text, with a snippet', () => {
    const list = [
      { id: 'a', locale: 'en', title: 'Scripts', body: 'Run JavaScript before a request.' },
      {
        id: 'b',
        locale: 'en',
        title: 'Tests',
        body: 'Assertions.\n\nTests can use **scripts** too.',
      },
      { id: 'c', locale: 'en', title: 'Sync', body: 'Nothing here.' },
    ];
    const found = searchTopics(list, 'script');
    expect(found.map((f) => f.id)).toEqual(['a', 'b']);
    expect(found[1]!.snippet).toContain('scripts');
    expect(found[1]!.snippet).not.toContain('**');
    expect(searchTopics(list, '  ')).toEqual([]);
    expect(searchTopics(list, 'JAVASCRIPT').map((f) => f.id)).toEqual(['a']);
  });
});
