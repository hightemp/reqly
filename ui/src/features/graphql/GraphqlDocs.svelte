<!-- Docs explorer: root operations, types with fields and arguments, search; click a type to open it. -->
<script lang="ts">
  import type { GraphQLSchema } from 'graphql';
  import { t } from '$lib/i18n';
  import IconButton from '$lib/ui/IconButton.svelte';
  import Input from '$lib/ui/Input.svelte';
  import { describeType, roots, search } from './docs';

  interface Props {
    schema: GraphQLSchema;
    onclose: () => void;
  }

  let { schema, onclose }: Props = $props();
  /** Navigation stack of type names; empty = roots. */
  let stack = $state<string[]>([]);
  let query = $state('');

  const current = $derived(stack.length ? schema.getType(stack[stack.length - 1]!) : null);
  const doc = $derived(current ? describeType(current, schema) : null);
  const found = $derived(search(schema, query));

  function open(name: string) {
    if (!schema.getType(name)) return;
    query = '';
    stack = [...stack, name];
  }
</script>

<aside class="docs" aria-label={t('graphql.docs')}>
  <header>
    {#if stack.length}
      <IconButton
        icon="arrow-up"
        label={t('graphql.back')}
        onclick={() => (stack = stack.slice(0, -1))}
      />
    {/if}
    <h3>{doc?.name ?? t('graphql.docs')}</h3>
    <span class="spacer"></span>
    <IconButton icon="close" label={t('graphql.closeDocs')} onclick={onclose} />
  </header>
  <div class="search">
    <Input
      value={query}
      placeholder={t('graphql.search')}
      aria-label={t('graphql.search')}
      oninput={(e) => (query = e.currentTarget.value)}
    />
  </div>
  <div class="content">
    {#if query.trim()}
      <ul class="list">
        {#each found as r, i (i)}
          <li>
            <button type="button" class="link" onclick={() => open(r.type)}
              >{r.type}{#if r.field}<span class="muted">.{r.field}</span>{/if}</button
            >
          </li>
        {:else}
          <li class="muted">{t('graphql.noResults')}</li>
        {/each}
      </ul>
    {:else if !doc}
      {#if schema.description}<p class="description">{schema.description}</p>{/if}
      <ul class="list">
        {#each roots(schema) as r (r.type)}
          <li>
            <span class="muted">{r.label}:</span>
            <button type="button" class="link type" onclick={() => open(r.type)}>{r.type}</button>
          </li>
        {/each}
      </ul>
    {:else}
      <p class="kind">{doc.kind}</p>
      {#if doc.description}<p class="description">{doc.description}</p>{/if}
      {#if doc.fields.length}
        <h4>{doc.kind === 'input' ? t('graphql.inputFields') : t('graphql.fields')}</h4>
        <ul class="fields">
          {#each doc.fields as f (f.name)}
            <li class:deprecated={f.deprecated !== null}>
              <code
                ><span class="field">{f.name}</span
                >{#if f.args.length}({#each f.args as a, i (a.name)}{#if i},
                    {/if}<span class="arg">{a.name}</span>:
                    <button type="button" class="link type" onclick={() => open(a.target)}
                      >{a.type}</button
                    >{#if a.defaultValue !== null}
                      = {a.defaultValue}{/if}{/each}){/if}:
                <button type="button" class="link type" onclick={() => open(f.target)}
                  >{f.type}</button
                ></code
              >
              {#if f.description}<p class="description">{f.description}</p>{/if}
              {#if f.deprecated}<p class="muted">
                  {t('graphql.deprecated', { reason: f.deprecated })}
                </p>{/if}
            </li>
          {/each}
        </ul>
      {/if}
      {#if doc.members.length}
        <h4>
          {doc.kind === 'enum'
            ? t('graphql.values')
            : doc.kind === 'interface'
              ? t('graphql.implementations')
              : t('graphql.possibleTypes')}
        </h4>
        <ul class="fields">
          {#each doc.members as m (m.name)}
            <li class:deprecated={m.deprecated !== null}>
              {#if doc.kind === 'enum'}
                <code class="field">{m.name}</code>
              {:else}
                <button type="button" class="link type" onclick={() => open(m.name)}
                  >{m.name}</button
                >
              {/if}
              {#if m.description}<p class="description">{m.description}</p>{/if}
            </li>
          {/each}
        </ul>
      {/if}
    {/if}
  </div>
</aside>

<style>
  .docs {
    display: flex;
    flex-direction: column;
    min-height: 0;
    height: 100%;
    border-left: 1px solid var(--c-border);
    background: var(--c-bg);
  }
  header {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    padding: var(--space-1) var(--space-2);
    border-bottom: 1px solid var(--c-border);
  }
  h3 {
    font-size: var(--font-size-sm);
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  h4 {
    margin: var(--space-3) 0 var(--space-1);
    font-size: var(--font-size-xs);
    text-transform: uppercase;
    color: var(--c-fg-subtle);
  }
  .spacer {
    flex: 1;
  }
  .search {
    padding: var(--space-2);
  }
  .content {
    flex: 1;
    overflow: auto;
    padding: 0 var(--space-3) var(--space-3);
    font-size: var(--font-size-sm);
  }
  .list,
  .fields {
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }
  code {
    font-family: var(--font-mono);
    font-size: var(--font-size-xs);
  }
  .field {
    color: var(--c-accent);
  }
  .arg {
    color: var(--c-fg-muted);
  }
  .link {
    color: var(--c-syntax-type, var(--c-warning, #d97706));
    font-family: var(--font-mono);
    font-size: var(--font-size-xs);
  }
  .link:hover {
    text-decoration: underline;
  }
  .kind,
  .muted {
    color: var(--c-fg-subtle);
    font-size: var(--font-size-xs);
  }
  .description {
    color: var(--c-fg-muted);
    margin-top: 2px;
  }
  .deprecated .field {
    text-decoration: line-through;
  }
</style>
