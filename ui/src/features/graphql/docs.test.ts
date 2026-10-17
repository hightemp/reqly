import { buildSchema } from 'graphql';
import { describe, expect, it } from 'vitest';
import { describeType, roots, search } from './docs';

const schema = buildSchema(`
  "A person"
  type User { id: ID! name(upper: Boolean = false): String friends: [User!]! old: Int @deprecated(reason: "gone") }
  enum Mood { HAPPY GRUMPY }
  union Thing = User
  input Filter { mood: Mood }
  type Query { user(id: ID!): User things(filter: Filter): [Thing] }
  type Subscription { ticks: Int }
`);

describe('graphql docs', () => {
  it('lists roots and describes types', () => {
    expect(roots(schema)).toEqual([
      { label: 'query', type: 'Query' },
      { label: 'subscription', type: 'Subscription' },
    ]);
    const user = describeType(schema.getType('User')!, schema);
    expect(user.description).toBe('A person');
    const name = user.fields.find((f) => f.name === 'name')!;
    expect(name.args[0]).toEqual({
      name: 'upper',
      type: 'Boolean',
      target: 'Boolean',
      defaultValue: 'false',
    });
    expect(user.fields.find((f) => f.name === 'friends')).toMatchObject({
      type: '[User!]!',
      target: 'User',
    });
    expect(user.fields.find((f) => f.name === 'old')?.deprecated).toBe('gone');
    expect(describeType(schema.getType('Mood')!, schema).members.map((m) => m.name)).toEqual([
      'HAPPY',
      'GRUMPY',
    ]);
    expect(describeType(schema.getType('Thing')!, schema).kind).toBe('union');
    expect(describeType(schema.getType('Filter')!, schema).kind).toBe('input');
  });

  it('searches types and fields', () => {
    expect(search(schema, 'user')).toEqual([
      { type: 'User', field: null },
      { type: 'Query', field: 'user' },
    ]);
  });
});
