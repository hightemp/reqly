<!-- Inline stroke icons (24×24 grid). Decorative unless a `label` is given. -->
<script lang="ts" module>
  const PATHS = {
    minimize: 'M5 12h14',
    maximize: 'M5 5h14v14H5z',
    restore: 'M8 8h11v11H8z M5 16V5h11',
    close: 'M6 6l12 12M18 6L6 18',
    sidebar: 'M4 5h16v14H4z M9 5v14',
    'layout-right': 'M4 5h16v14H4z M13 5v14',
    'layout-bottom': 'M4 5h16v14H4z M4 13h16',
    sun: 'M12 4v2M12 18v2M4 12h2M18 12h2M6.3 6.3l1.4 1.4M16.3 16.3l1.4 1.4M6.3 17.7l1.4-1.4M16.3 7.7l1.4-1.4 M12 8a4 4 0 1 0 0 8a4 4 0 1 0 0-8z',
    moon: 'M20 14.5A8 8 0 0 1 9.5 4a8 8 0 1 0 10.5 10.5z',
    monitor: 'M4 5h16v11H4z M9 20h6 M12 16v4',
    globe:
      'M12 3a9 9 0 1 0 0 18a9 9 0 1 0 0-18z M3 12h18 M12 3c3 3.5 3 14.5 0 18 M12 3c-3 3.5-3 14.5 0 18',
    plug: 'M9 7V3M15 7V3M6 7h12v4a6 6 0 0 1-12 0z M12 17v4',
  } as const;
  export type IconName = keyof typeof PATHS;
</script>

<script lang="ts">
  interface Props {
    name: IconName;
    size?: number;
    label?: string;
  }
  let { name, size = 16, label }: Props = $props();
</script>

<svg
  class="icon"
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="1.75"
  stroke-linecap="round"
  stroke-linejoin="round"
  role={label ? 'img' : undefined}
  aria-label={label}
  aria-hidden={label ? undefined : 'true'}
>
  <path d={PATHS[name]} />
</svg>

<style>
  .icon {
    flex: none;
    display: block;
  }
</style>
