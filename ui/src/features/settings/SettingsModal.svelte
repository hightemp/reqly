<!-- Settings: section navigation on the left, content on the right, with search. -->
<script lang="ts">
  import { t } from '$lib/i18n';
  import type { Services } from '$lib/services.svelte';
  import Modal from '$lib/ui/overlay/Modal.svelte';
  import About from './sections/About.svelte';
  import Account from './sections/Account.svelte';
  import Admin from './sections/Admin.svelte';
  import Appearance from './sections/Appearance.svelte';
  import Certificates from './sections/Certificates.svelte';
  import CustomThemes from './sections/CustomThemes.svelte';
  import General from './sections/General.svelte';
  import History from './sections/History.svelte';
  import Http from './sections/Http.svelte';
  import Keybindings from './sections/Keybindings.svelte';
  import Templates from './sections/Templates.svelte';

  type Section =
    | 'general'
    | 'appearance'
    | 'themes'
    | 'http'
    | 'templates'
    | 'certificates'
    | 'history'
    | 'keybindings'
    | 'account'
    | 'admin'
    | 'about';

  let {
    services,
    onclose,
    initial = 'general',
  }: { services: Services; onclose: () => void; initial?: Section } = $props();
  // svelte-ignore state_referenced_locally
  let section = $state<Section>(initial);
  let query = $state('');

  // Words each section is found by (labels of its fields).
  const sections = $derived<{ id: Section; label: string; keywords: string }[]>([
    {
      id: 'general',
      label: t('settings.general'),
      keywords: [
        t('settings.language'),
        t('settings.autosave'),
        t('settings.sidebarMode'),
        t('settings.responseLayout'),
      ].join(' '),
    },
    {
      id: 'appearance',
      label: t('settings.appearance'),
      keywords: [
        t('settings.theme'),
        t('settings.density'),
        t('settings.zoom'),
        t('settings.uiFont'),
        t('settings.editorFont'),
      ].join(' '),
    },
    { id: 'themes', label: t('settings.customThemes'), keywords: t('themes.import') },
    {
      id: 'http',
      label: t('settings.http'),
      keywords: [
        t('settings.proxy'),
        t('requestSettings.timeout'),
        t('requestSettings.verifyTls'),
      ].join(' '),
    },
    {
      id: 'templates',
      label: t('settings.templates'),
      keywords: t('settings.fakerLocale'),
    },
    {
      id: 'certificates',
      label: t('settings.certificates'),
      keywords: [t('certs.ca'), t('certs.client')].join(' '),
    },
    { id: 'history', label: t('settings.history'), keywords: t('settings.historyPerRequest') },
    { id: 'keybindings', label: t('settings.keybindings'), keywords: t('hotkeys.change') },
    ...(services.account.user
      ? [
          {
            id: 'account' as const,
            label: t('settings.account'),
            keywords: [t('account.password'), t('account.tokens')].join(' '),
          },
        ]
      : []),
    ...(services.account.user?.isAdmin
      ? [
          {
            id: 'admin' as const,
            label: t('settings.admin'),
            keywords: [t('admin.users'), t('admin.workspaces'), t('admin.server')].join(' '),
          },
        ]
      : []),
    {
      id: 'about',
      label: t('settings.about'),
      keywords: [t('settings.version'), t('settings.cleanup')].join(' '),
    },
  ]);
  const visible = $derived(
    query.trim()
      ? sections.filter((s) =>
          `${s.label} ${s.keywords}`.toLowerCase().includes(query.trim().toLowerCase()),
        )
      : sections,
  );
  $effect(() => {
    if (visible.length && !visible.some((s) => s.id === section) && section !== 'keybindings')
      section = visible[0]!.id;
  });
</script>

<Modal title={t('settings.title')} width="min(980px, 96vw)" {onclose}>
  <div class="layout">
    <nav class="nav" aria-label={t('settings.title')}>
      <input
        class="search"
        bind:value={query}
        spellcheck="false"
        aria-label={t('settings.search')}
        placeholder={t('settings.search')}
      />
      {#each visible as s (s.id)}
        <button
          type="button"
          class="nav-item"
          aria-current={section === s.id ? 'page' : undefined}
          onclick={() => (section = s.id)}>{s.label}</button
        >
      {/each}
    </nav>
    <div class="content">
      {#if section === 'general'}
        <General {services} />
      {:else if section === 'appearance'}
        <Appearance {services} />
      {:else if section === 'themes'}
        <CustomThemes {services} />
      {:else if section === 'http'}
        <Http {services} />
      {:else if section === 'templates'}
        <Templates {services} />
      {:else if section === 'certificates'}
        <Certificates {services} />
      {:else if section === 'history'}
        <History {services} />
      {:else if section === 'account'}
        <Account {services} />
      {:else if section === 'admin'}
        <Admin {services} />
      {:else if section === 'keybindings'}
        <Keybindings {services} filter={query} />
      {:else}
        <About {services} />
      {/if}
    </div>
  </div>
</Modal>

<style>
  .layout {
    display: grid;
    grid-template-columns: 200px 1fr;
    gap: var(--space-4);
    height: min(640px, 72vh);
  }
  .nav {
    display: flex;
    flex-direction: column;
    gap: 2px;
    border-right: 1px solid var(--c-border);
    padding-right: var(--space-3);
  }
  .search {
    height: var(--control-height);
    padding: 0 var(--space-2);
    margin-bottom: var(--space-2);
    border-radius: var(--radius-sm);
    border: 1px solid var(--c-border);
    background: var(--c-bg-sunken);
    color: var(--c-fg);
    outline: none;
  }
  .search:focus {
    border-color: var(--c-accent);
  }
  .nav-item {
    text-align: left;
    padding: 6px var(--space-2);
    border-radius: var(--radius-sm);
    color: var(--c-fg-muted);
  }
  .nav-item:hover {
    background: var(--c-bg-hover);
    color: var(--c-fg);
  }
  .nav-item[aria-current='page'] {
    background: var(--c-bg-selected);
    color: var(--c-fg);
  }
  .content {
    overflow: auto;
    padding-right: var(--space-2);
  }
</style>
