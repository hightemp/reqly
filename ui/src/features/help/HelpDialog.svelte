<!--
  Help window: contents and search on the left, the page on the right.
  `help:` links in a page open other pages; Back returns.
-->
<script lang="ts">
  import { tick } from 'svelte';
  import { formatCombo, type HotkeyRegistry, type Platform } from '$lib/hotkeys/hotkeys';
  import { i18n, t } from '$lib/i18n';
  import { renderMarkdown } from '$lib/markdown';
  import Button from '$lib/ui/Button.svelte';
  import Input from '$lib/ui/Input.svelte';
  import Kbd from '$lib/ui/Kbd.svelte';
  import Modal from '$lib/ui/overlay/Modal.svelte';
  import { searchTopics, topic, topics } from './topics';

  interface Props {
    hotkeys: HotkeyRegistry;
    platform: Platform;
    /** `topic` or `topic#anchor`. */
    page: string;
    onclose: () => void;
  }

  let { hotkeys, platform, page, onclose }: Props = $props();

  // svelte-ignore state_referenced_locally
  let current = $state(page);
  let history = $state<string[]>([]);
  let query = $state('');
  let articleEl: HTMLElement | null = $state(null);

  const list = $derived(topics(i18n.locale));
  const id = $derived(current.split('#')[0]!);
  const shown = $derived(topic(i18n.locale, id) ?? list[0]!);
  const html = $derived(renderMarkdown(shown.body, { help: true }));
  const hits = $derived(searchTopics(list, query));

  // i18n-dynamic: hotkeys.
  function label(action: string) {
    const goto = /^tab\.goto(\d)$/.exec(action);
    return goto ? t('hotkeys.tabGoto', { n: goto[1]! }) : t(`hotkeys.${action}` as never);
  }

  const shortcuts = $derived(
    shown.id === 'shortcuts'
      ? hotkeys.definitions().map((a) => ({
          id: a.id,
          label: label(a.id),
          combos: hotkeys.combosFor(a.id).map((c) => formatCombo(c, platform)),
        }))
      : [],
  );

  async function show(next: string, remember = true) {
    if (remember && next !== current) history = [...history, current];
    current = next;
    await tick();
    const anchor = next.split('#')[1];
    const target = anchor ? articleEl?.querySelector(`#${CSS.escape(anchor)}`) : null;
    if (target) target.scrollIntoView?.({ block: 'start' });
    else articleEl?.scrollTo?.({ top: 0 });
  }

  function back() {
    const previous = history.at(-1);
    if (!previous) return;
    history = history.slice(0, -1);
    void show(previous, false);
  }

  function onArticleClick(e: MouseEvent) {
    const link = (e.target as HTMLElement).closest<HTMLAnchorElement>('a[data-help]');
    if (!link) return;
    e.preventDefault();
    void show(link.dataset.help!);
  }

  // Links in the rendered page (delegated: the page is HTML from Markdown).
  $effect(() => {
    const el = articleEl;
    el?.addEventListener('click', onArticleClick);
    return () => el?.removeEventListener('click', onArticleClick);
  });

  $effect(() => {
    // Scroll to the anchor of the page the window was opened with.
    if (articleEl && page.includes('#')) void show(page, false);
  });
</script>

<Modal title={t('help.title')} width="min(980px, calc(100vw - 32px))" {onclose}>
  <div class="help">
    <aside class="side">
      <Input
        type="search"
        bind:value={query}
        placeholder={t('help.search')}
        aria-label={t('help.search')}
      />
      {#if query.trim()}
        <nav aria-label={t('help.results')}>
          {#each hits as hit (hit.id)}
            <a
              href="#{hit.id}"
              class="hit"
              onclick={(e) => {
                e.preventDefault();
                void show(hit.id);
              }}
            >
              <span class="hit-title">{hit.title}</span>
              {#if hit.snippet}<span class="snippet">{hit.snippet}</span>{/if}
            </a>
          {:else}
            <p class="empty">{t('help.nothing')}</p>
          {/each}
        </nav>
      {:else}
        <nav aria-label={t('help.contents')}>
          {#each list as item (item.id)}
            <a
              href="#{item.id}"
              class="toc"
              aria-current={item.id === shown.id ? 'page' : undefined}
              onclick={(e) => {
                e.preventDefault();
                void show(item.id);
              }}>{item.title}</a
            >
          {/each}
        </nav>
      {/if}
    </aside>
    <article class="page" bind:this={articleEl}>
      <h1>{shown.title}</h1>
      <!-- eslint-disable-next-line svelte/no-at-html-tags -- renderMarkdown escapes the bundled pages -->
      {@html html}
      {#if shortcuts.length}
        <table aria-label={t('help.shortcuts')}>
          <tbody>
            {#each shortcuts as row (row.id)}
              <tr>
                <td>{row.label}</td>
                <td>
                  <span class="keys">
                    {#each row.combos as combo (combo)}<Kbd keys={combo} />{/each}
                  </span>
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      {/if}
    </article>
  </div>
  {#snippet footer()}
    <Button disabled={history.length === 0} onclick={back}>{t('help.back')}</Button>
    <span class="grow"></span>
    <Button variant="primary" onclick={onclose}>{t('common.close')}</Button>
  {/snippet}
</Modal>

<style>
  .help {
    display: grid;
    grid-template-columns: 220px minmax(0, 1fr);
    gap: var(--space-4);
    height: min(70vh, 720px);
  }
  .side {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    min-height: 0;
  }
  nav {
    display: flex;
    flex-direction: column;
    overflow: auto;
    min-height: 0;
  }
  .toc,
  .hit {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: var(--space-1) var(--space-2);
    border-radius: var(--radius-sm);
    color: var(--c-fg);
    text-decoration: none;
    font-size: var(--font-size-sm);
  }
  .toc:hover,
  .hit:hover {
    background: var(--c-bg-hover);
  }
  .toc[aria-current='page'] {
    background: var(--c-accent-subtle);
    color: var(--c-accent);
  }
  .snippet {
    color: var(--c-fg-muted);
    font-size: var(--font-size-xs);
  }
  .empty {
    margin: var(--space-2);
    color: var(--c-fg-muted);
    font-size: var(--font-size-sm);
  }
  .page {
    overflow: auto;
    min-width: 0;
    min-height: 0;
    overflow-wrap: anywhere;
    padding-right: var(--space-2);
    font-size: var(--font-size-sm);
    line-height: 1.55;
  }
  .page h1 {
    margin: 0 0 var(--space-3);
    font-size: 1.4em;
  }
  .page :global(h2) {
    margin: var(--space-4) 0 var(--space-2);
    font-size: 1.15em;
  }
  .page :global(h3) {
    margin: var(--space-3) 0 var(--space-1);
    font-size: 1em;
  }
  .page :global(p),
  .page :global(ul),
  .page :global(ol) {
    margin: 0 0 var(--space-2);
  }
  .page :global(code) {
    padding: 0 3px;
    border-radius: var(--radius-sm);
    background: var(--c-bg-sunken);
    font-family: var(--font-mono);
    font-size: 0.92em;
  }
  .page :global(pre) {
    margin: 0 0 var(--space-3);
    padding: var(--space-2);
    border-radius: var(--radius-sm);
    background: var(--c-bg-sunken);
    overflow: auto;
  }
  .page :global(pre code) {
    padding: 0;
    background: none;
  }
  .page :global(table) {
    width: 100%;
    margin: 0 0 var(--space-3);
    border-collapse: collapse;
  }
  .page :global(th),
  .page :global(td) {
    padding: var(--space-1) var(--space-2);
    border-bottom: 1px solid var(--c-border);
    text-align: left;
    vertical-align: top;
  }
  .page :global(a) {
    color: var(--c-accent);
  }
  .keys {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1);
  }
  .grow {
    flex: 1;
  }
  @media (max-width: 700px) {
    .help {
      grid-template-columns: minmax(0, 1fr);
      grid-template-rows: auto 1fr;
      height: 75vh;
    }
    .side nav {
      max-height: 30vh;
    }
  }
</style>
