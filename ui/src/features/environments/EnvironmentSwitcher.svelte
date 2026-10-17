<!-- Active environment picker shown in the sidebar header. -->
<script lang="ts">
  import { formatCombo } from '$lib/hotkeys/hotkeys';
  import { t } from '$lib/i18n';
  import type { Services } from '$lib/services.svelte';
  import Icon from '$lib/ui/Icon.svelte';
  import { menus, type MenuEntry } from '$lib/ui/overlay/menus.svelte';

  let { services, onmanage }: { services: Services; onmanage: () => void } = $props();
  const envs = $derived(services.environments);

  const label = $derived(
    envs.active?.name ?? (envs.activeGlobal ? envs.activeGlobal.name : t('environments.none')),
  );
  const color = $derived(envs.active?.color ?? envs.activeGlobal?.color ?? null);

  function open(anchor: HTMLElement) {
    const combo = services.hotkeys.combosFor('environments.open')[0];
    const entries: MenuEntry[] = [
      { heading: t('environments.collection') },
      {
        id: 'none',
        label: t('environments.none'),
        checked: envs.activeId === null,
        onSelect: () => void envs.setActive(null),
      },
      ...envs.subs.map((e) => ({
        id: `sub-${e.id}`,
        label: e.name,
        checked: envs.activeId === e.id,
        onSelect: () => void envs.setActive(e.id),
      })),
    ];
    if (envs.global.length > 0) {
      entries.push(
        { heading: t('environments.global') },
        {
          id: 'global-none',
          label: t('environments.none'),
          checked: envs.activeGlobalId === null,
          onSelect: () => void envs.setActiveGlobal(null),
        },
        ...envs.global.map((e) => ({
          id: `global-${e.id}`,
          label: e.name,
          checked: envs.activeGlobalId === e.id,
          onSelect: () => void envs.setActiveGlobal(e.id),
        })),
      );
    }
    entries.push(
      { separator: true },
      {
        id: 'manage',
        label: t('environments.manage'),
        shortcut: combo ? formatCombo(combo, services.platform) : undefined,
        onSelect: onmanage,
      },
    );
    menus.openBelow(anchor, entries);
  }
</script>

<button
  type="button"
  class="switcher"
  aria-haspopup="menu"
  aria-label="{t('environments.label')}: {label}"
  onclick={(e) => open(e.currentTarget)}
>
  <Icon name="layers" size={14} />
  <span class="dot" style:background={color ?? 'transparent'} class:empty={!color}></span>
  <span class="name">{label}</span>
  <Icon name="more" size={14} />
</button>

<style>
  .switcher {
    width: 100%;
    display: flex;
    align-items: center;
    gap: var(--space-2);
    height: var(--control-height);
    padding: 0 var(--space-2);
    border-radius: var(--radius-sm);
    color: var(--c-fg-muted);
    font-size: var(--font-size-sm);
  }
  .switcher:hover {
    background: var(--c-bg-hover);
    color: var(--c-fg);
  }
  .dot {
    width: 8px;
    height: 8px;
    flex: none;
    border-radius: 50%;
  }
  .dot.empty {
    display: none;
  }
  .name {
    flex: 1;
    min-width: 0;
    text-align: left;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
