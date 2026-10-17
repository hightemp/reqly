// Docs explorer model: what to show for a schema type and how to name types.

import {
  getNamedType,
  isEnumType,
  isInputObjectType,
  isInterfaceType,
  isObjectType,
  isScalarType,
  isUnionType,
  type GraphQLField,
  type GraphQLInputField,
  type GraphQLNamedType,
  type GraphQLSchema,
  type GraphQLType,
} from 'graphql';

export interface DocField {
  name: string;
  /** e.g. `[User!]!` */
  type: string;
  /** Named type to navigate to. */
  target: string;
  description: string;
  args: { name: string; type: string; target: string; defaultValue: string | null }[];
  deprecated: string | null;
}

export interface DocType {
  name: string;
  kind: 'object' | 'interface' | 'union' | 'enum' | 'input' | 'scalar';
  description: string;
  fields: DocField[];
  /** Union members, implementations or enum values. */
  members: { name: string; description: string; deprecated: string | null }[];
}

const named = (t: GraphQLType) => getNamedType(t).name;

function field(f: GraphQLField<unknown, unknown> | GraphQLInputField): DocField {
  const args = 'args' in f ? f.args : [];
  return {
    name: f.name,
    type: String(f.type),
    target: named(f.type),
    description: f.description ?? '',
    args: args.map((a) => ({
      name: a.name,
      type: String(a.type),
      target: named(a.type),
      defaultValue: a.defaultValue === undefined ? null : JSON.stringify(a.defaultValue),
    })),
    deprecated: 'deprecationReason' in f ? (f.deprecationReason ?? null) : null,
  };
}

export function describeType(type: GraphQLNamedType, schema: GraphQLSchema): DocType {
  const base = {
    name: type.name,
    description: type.description ?? '',
    fields: [] as DocField[],
    members: [] as DocType['members'],
  };
  if (isObjectType(type) || isInterfaceType(type)) {
    const members = isInterfaceType(type)
      ? schema
          .getPossibleTypes(type)
          .map((t) => ({ name: t.name, description: t.description ?? '', deprecated: null }))
      : [];
    return {
      ...base,
      kind: isObjectType(type) ? 'object' : 'interface',
      fields: Object.values(type.getFields()).map(field),
      members,
    };
  }
  if (isInputObjectType(type))
    return { ...base, kind: 'input', fields: Object.values(type.getFields()).map(field) };
  if (isUnionType(type))
    return {
      ...base,
      kind: 'union',
      members: type
        .getTypes()
        .map((t) => ({ name: t.name, description: t.description ?? '', deprecated: null })),
    };
  if (isEnumType(type))
    return {
      ...base,
      kind: 'enum',
      members: type.getValues().map((v) => ({
        name: v.name,
        description: v.description ?? '',
        deprecated: v.deprecationReason ?? null,
      })),
    };
  void isScalarType;
  return { ...base, kind: 'scalar' };
}

/** Root operation types that exist. */
export function roots(schema: GraphQLSchema): { label: string; type: string }[] {
  return [
    ['query', schema.getQueryType()],
    ['mutation', schema.getMutationType()],
    ['subscription', schema.getSubscriptionType()],
  ]
    .filter(([, t]) => t)
    .map(([label, t]) => ({ label: label as string, type: (t as GraphQLNamedType).name }));
}

/** Types and fields whose name contains `query` (case-insensitive). */
export function search(schema: GraphQLSchema, query: string, limit = 50) {
  const q = query.trim().toLowerCase();
  if (!q) return [];
  const out: { type: string; field: string | null }[] = [];
  for (const t of Object.values(schema.getTypeMap())) {
    if (t.name.startsWith('__')) continue;
    if (t.name.toLowerCase().includes(q)) out.push({ type: t.name, field: null });
    if (isObjectType(t) || isInterfaceType(t) || isInputObjectType(t)) {
      for (const f of Object.values(t.getFields()))
        if (f.name.toLowerCase().includes(q)) out.push({ type: t.name, field: f.name });
    }
    if (out.length >= limit) break;
  }
  return out.slice(0, limit);
}
