// GraphQL schemas per request: cached in the core, built into graphql-js
// schemas here for the editor and the docs explorer.

import {
  buildClientSchema,
  buildSchema,
  type GraphQLSchema,
  type IntrospectionQuery,
} from 'graphql';
import type { GraphqlSchema } from '$lib/api/generated/GraphqlSchema';
import type { Transport } from '$lib/api/transport';
import { errorMessage } from '$lib/errors';

export class SchemaEntry {
  record = $state<GraphqlSchema | null>(null);
  schema = $state.raw<GraphQLSchema | null>(null);
  error = $state<string | null>(null);
  loading = $state(false);
  loaded = false;
}

/** Builds a graphql-js schema from a stored one (introspection JSON or SDL). */
export function buildFrom(record: GraphqlSchema): GraphQLSchema {
  if (record.format === 'sdl') return buildSchema(record.schema);
  const json = JSON.parse(record.schema) as IntrospectionQuery | { data: IntrospectionQuery };
  return buildClientSchema('data' in json ? json.data : json);
}

export class SchemaStore {
  // eslint-disable-next-line svelte/prefer-svelte-reactivity -- entries are reactive themselves
  private entries = new Map<string, SchemaEntry>();

  constructor(
    private readonly transport: Transport,
    private readonly flush: (id: string) => Promise<void>,
  ) {}

  entry(requestId: string): SchemaEntry {
    let e = this.entries.get(requestId);
    if (!e) {
      e = new SchemaEntry();
      this.entries.set(requestId, e);
    }
    return e;
  }

  private adopt(e: SchemaEntry, record: GraphqlSchema | null) {
    e.record = record;
    e.error = null;
    try {
      e.schema = record ? buildFrom(record) : null;
    } catch (err) {
      e.schema = null;
      e.error = errorMessage(err);
    }
  }

  async load(requestId: string): Promise<SchemaEntry> {
    const e = this.entry(requestId);
    if (e.loaded) return e;
    e.loaded = true;
    try {
      const { schema } = await this.transport.call('graphql.schemaGet', { requestId });
      this.adopt(e, schema ?? null);
    } catch (err) {
      e.error = errorMessage(err);
    }
    return e;
  }

  async introspect(requestId: string) {
    const e = this.entry(requestId);
    e.loading = true;
    try {
      await this.flush(requestId);
      this.adopt(e, await this.transport.call('graphql.introspect', { requestId }));
    } catch (err) {
      e.error = errorMessage(err);
    } finally {
      e.loading = false;
    }
  }

  async fromFile(requestId: string, file: File) {
    const e = this.entry(requestId);
    const text = await file.text();
    const format =
      file.name.endsWith('.json') || text.trimStart().startsWith('{') ? 'introspection' : 'sdl';
    try {
      this.adopt(
        e,
        await this.transport.call('graphql.schemaSet', {
          requestId,
          format,
          source: file.name,
          schema: text,
        }),
      );
    } catch (err) {
      e.error = errorMessage(err);
    }
  }

  async clear(requestId: string) {
    const e = this.entry(requestId);
    try {
      await this.transport.call('graphql.schemaClear', { requestId });
      this.adopt(e, null);
    } catch (err) {
      e.error = errorMessage(err);
    }
  }
}

const stores = new WeakMap<Transport, SchemaStore>();

export function schemaStore(
  transport: Transport,
  flush: (id: string) => Promise<void>,
): SchemaStore {
  let s = stores.get(transport);
  if (!s) {
    s = new SchemaStore(transport, flush);
    stores.set(transport, s);
  }
  return s;
}
