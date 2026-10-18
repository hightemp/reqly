<!-- GraphQL body: query editor with schema completion and validation, variables, operation, schema and docs. -->
<script lang="ts">
  import { graphql as graphqlLanguage, updateSchema } from 'cm6-graphql';
  import { OperationTypeNode, parse, print } from 'graphql';
  import type { GraphqlBody } from '$lib/api/generated/GraphqlBody';
  import { errorMessage } from '$lib/errors';
  import { i18n, t } from '$lib/i18n';
  import type { Services } from '$lib/services.svelte';
  import Button from '$lib/ui/Button.svelte';
  import CodeEditor from '$lib/ui/code/CodeEditor.svelte';
  import { menus } from '$lib/ui/overlay/menus.svelte';
  import Select from '$lib/ui/overlay/Select.svelte';
  import { toasts } from '$lib/ui/overlay/toasts.svelte';
  import Spinner from '$lib/ui/Spinner.svelte';
  import GraphqlDocs from './GraphqlDocs.svelte';
  import { schemaStore } from './schema.svelte';

  interface Props {
    services: Services;
    nodeId: string;
    value: GraphqlBody;
    onchange: (value: GraphqlBody) => void;
  }

  let { services, nodeId, value, onchange }: Props = $props();
  let queryEditor: CodeEditor | undefined = $state();
  let fileInput: HTMLInputElement | undefined = $state();
  let docsOpen = $state(false);

  const store = $derived(schemaStore(services.transport, (id) => services.editor.flush(id)));
  const entry = $derived(store.entry(nodeId));
  // The language extension is created once per editor; schemas are pushed in.
  const language = graphqlLanguage();

  $effect(() => {
    void store.load(nodeId);
  });

  $effect(() => {
    const view = queryEditor?.getView();
    const schema = entry.schema;
    if (view) updateSchema(view, schema ?? undefined);
  });

  /** Named operations in the document (for the operation picker). */
  const operations = $derived.by(() => {
    try {
      return parse(value.query)
        .definitions.filter((d) => d.kind === 'OperationDefinition')
        .map((d) => ({
          name: d.kind === 'OperationDefinition' ? (d.name?.value ?? '') : '',
          type: d.kind === 'OperationDefinition' ? d.operation : OperationTypeNode.QUERY,
        }));
    } catch {
      return [];
    }
  });

  function prettify() {
    try {
      onchange({ ...value, query: print(parse(value.query)) });
    } catch (e) {
      toasts.show({
        message: t('graphql.parseError', { message: errorMessage(e) }),
        kind: 'warning',
      });
    }
  }

  function schemaMenu(anchor: HTMLElement) {
    menus.openBelow(anchor, [
      {
        id: 'fetch',
        label: t('graphql.fetchSchema'),
        onSelect: () => void store.introspect(nodeId),
      },
      { id: 'file', label: t('graphql.loadSchema'), onSelect: () => fileInput?.click() },
      ...(entry.record
        ? [
            {
              id: 'clear',
              label: t('graphql.clearSchema'),
              danger: true,
              onSelect: () => void store.clear(nodeId),
            },
          ]
        : []),
    ]);
  }

  const schemaLabel = $derived(
    entry.record
      ? t('graphql.schemaFrom', {
          source: entry.record.source,
          time: i18n.formatDateTime(entry.record.fetchedAt, {
            dateStyle: 'short',
            timeStyle: 'short',
          }),
        })
      : t('graphql.noSchema'),
  );
</script>

<div class="gql">
  <div class="bar">
    <Button size="sm" variant="ghost" onclick={prettify}>{t('graphql.prettify')}</Button>
    {#if operations.filter((o) => o.name).length > 1}
      <Select
        label={t('graphql.operation')}
        compact
        value={value.operationName}
        options={[
          { value: '', label: t('graphql.firstOperation') },
          ...operations
            .filter((o) => o.name)
            .map((o) => ({ value: o.name, label: `${o.type} ${o.name}` })),
        ]}
        onchange={(operationName) => onchange({ ...value, operationName })}
      />
    {/if}
    <span class="spacer"></span>
    {#if entry.loading}<Spinner size={12} />{/if}
    <button
      type="button"
      class="schema"
      class:error={entry.error}
      title={entry.error ?? schemaLabel}
      onclick={(e) => schemaMenu(e.currentTarget)}
      >{entry.error ? t('graphql.schemaError') : schemaLabel}</button
    >
    <Button
      size="sm"
      variant={docsOpen ? 'default' : 'ghost'}
      disabled={!entry.schema}
      onclick={() => (docsOpen = !docsOpen)}>{t('graphql.docs')}</Button
    >
    <input
      bind:this={fileInput}
      type="file"
      accept=".graphql,.graphqls,.gql,.json"
      hidden
      onchange={(e) => {
        const f = e.currentTarget.files?.[0];
        if (f) void store.fromFile(nodeId, f);
        e.currentTarget.value = '';
      }}
    />
  </div>
  {#if entry.error}<p class="error-text" role="alert">{entry.error}</p>{/if}
  <div class="main" class:with-docs={docsOpen && entry.schema}>
    <div class="editors">
      <div class="query">
        <CodeEditor
          bind:this={queryEditor}
          label={t('graphql.query')}
          {nodeId}
          collab="body.graphql.query"
          value={value.query}
          extensions={[language]}
          onchange={(query) => onchange({ ...value, query })}
        />
      </div>
      <div class="vars-head">{t('graphql.variables')}</div>
      <div class="vars">
        <CodeEditor
          label={t('graphql.variables')}
          language="json"
          templates
          {nodeId}
          lineNumbers={false}
          placeholder={'{ "id": 1 }'}
          value={value.variables}
          collab="body.graphql.variables"
          onchange={(variables) => onchange({ ...value, variables })}
        />
      </div>
    </div>
    {#if docsOpen && entry.schema}
      <GraphqlDocs schema={entry.schema} onclose={() => (docsOpen = false)} />
    {/if}
  </div>
</div>

<style>
  .gql {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    height: 100%;
    min-height: 0;
  }
  .bar {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .spacer {
    flex: 1;
  }
  .schema {
    max-width: 320px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--c-fg-muted);
    font-size: var(--font-size-xs);
    padding: 2px var(--space-2);
    border-radius: var(--radius-sm);
  }
  .schema:hover {
    background: var(--c-bg-hover);
  }
  .schema.error,
  .error-text {
    color: var(--c-danger);
    font-size: var(--font-size-xs);
  }
  .main {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: 1fr;
  }
  .main.with-docs {
    grid-template-columns: 1fr minmax(240px, 34%);
  }
  .editors {
    display: grid;
    grid-template-rows: 2fr auto 1fr;
    min-height: 0;
    border: 1px solid var(--c-border);
    border-radius: var(--radius-sm);
    overflow: hidden;
  }
  .query,
  .vars {
    min-height: 0;
  }
  .vars-head {
    padding: 2px var(--space-2);
    font-size: var(--font-size-xs);
    color: var(--c-fg-subtle);
    border-top: 1px solid var(--c-border);
    border-bottom: 1px solid var(--c-border);
    background: var(--c-bg-sunken);
  }
</style>
