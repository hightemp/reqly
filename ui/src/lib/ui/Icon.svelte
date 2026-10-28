<!-- Inline stroke icons (24×24 grid). Decorative unless a `label` is given. -->
<script lang="ts" module>
  const PATHS = {
    minimize: 'M5 12h14',
    maximize: 'M5 5h14v14H5z',
    restore: 'M8 8h11v11H8z M5 16V5h11',
    close: 'M6 6l12 12M18 6L6 18',
    sidebar: 'M4 5h16v14H4z M9 5v14',
    menu: 'M4 6h16M4 12h16M4 18h16',
    'layout-right': 'M4 5h16v14H4z M13 5v14',
    'layout-bottom': 'M4 5h16v14H4z M4 13h16',
    sun: 'M12 4v2M12 18v2M4 12h2M18 12h2M6.3 6.3l1.4 1.4M16.3 16.3l1.4 1.4M6.3 17.7l1.4-1.4M16.3 7.7l1.4-1.4 M12 8a4 4 0 1 0 0 8a4 4 0 1 0 0-8z',
    moon: 'M20 14.5A8 8 0 0 1 9.5 4a8 8 0 1 0 10.5 10.5z',
    monitor: 'M4 5h16v11H4z M9 20h6 M12 16v4',
    globe:
      'M12 3a9 9 0 1 0 0 18a9 9 0 1 0 0-18z M3 12h18 M12 3c3 3.5 3 14.5 0 18 M12 3c-3 3.5-3 14.5 0 18',
    plug: 'M9 7V3M15 7V3M6 7h12v4a6 6 0 0 1-12 0z M12 17v4',
    folder: 'M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z',
    'folder-open': 'M3 7a2 2 0 0 1 2-2h4l2 2h7a2 2 0 0 1 2 2v1 M3 7v10a2 2 0 0 0 2 2h12l3-8H7l-3 8',
    plus: 'M12 5v14M5 12h14',
    search: 'M11 4a7 7 0 1 0 0 14a7 7 0 1 0 0-14z M20 20l-4-4',
    more: 'M5 12h.01M12 12h.01M19 12h.01',
    tree: 'M4 5h6 M8 5v14 M8 12h6 M8 19h6 M14 9h6v6h-6z',
    list: 'M8 6h12M8 12h12M8 18h12M4 6h.01M4 12h.01M4 18h.01',
    send: 'M5 12h14 M13 6l6 6-6 6',
    play: 'M7 5l12 7-12 7z',
    stop: 'M7 7h10v10H7z',
    copy: 'M9 9h10v10H9z M5 15V5h10',
    download: 'M12 4v12 M7 11l5 5 5-5 M5 20h14',
    trash: 'M4 7h16 M9 7V4h6v3 M6 7l1 13h10l1-13',
    history: 'M3 12a9 9 0 1 0 3-6.7 M3 4v5h5 M12 7v5l3 3',
    cookie:
      'M12 3a9 9 0 1 0 9 9a3 3 0 0 1-3-3a3 3 0 0 1-3-3a3 3 0 0 1-3-3z M8 13h.01 M15 15h.01 M10 17h.01',
    settings:
      'M12 9a3 3 0 1 0 0 6a3 3 0 1 0 0-6z M19 12a7 7 0 0 0-.1-1.2l2-1.6-2-3.4-2.4 1a7 7 0 0 0-2-1.2L14 3h-4l-.5 2.6a7 7 0 0 0-2 1.2l-2.4-1-2 3.4 2 1.6a7 7 0 0 0 0 2.4l-2 1.6 2 3.4 2.4-1a7 7 0 0 0 2 1.2L10 21h4l.5-2.6a7 7 0 0 0 2-1.2l2.4 1 2-3.4-2-1.6c.1-.4.1-.8.1-1.2z',
    wrap: 'M4 6h16 M4 12h13a3 3 0 0 1 0 6h-4 M15 16l-2 2 2 2 M4 18h5',
    pin: 'M9 3h6l-1 6 4 4H6l4-4z M12 13v8',
    lock: 'M6 11h12v9H6z M8 11V8a4 4 0 0 1 8 0v3',
    unlock: 'M6 11h12v9H6z M8 11V8a4 4 0 0 1 7.5-2',
    layers: 'M12 3l9 5-9 5-9-5z M3 13l9 5 9-5',
    braces:
      'M9 4H8a2 2 0 0 0-2 2v4l-2 2 2 2v4a2 2 0 0 0 2 2h1 M15 4h1a2 2 0 0 1 2 2v4l2 2-2 2v4a2 2 0 0 1-2 2h-1',
    'arrow-up': 'M12 19V5 M6 11l6-6 6 6',
    'arrow-down': 'M12 5v14 M6 13l6 6 6-6',
    info: 'M12 3a9 9 0 1 0 0 18a9 9 0 1 0 0-18z M12 11v5 M12 8h.01',
    book: 'M5 4h9a3 3 0 0 1 3 3v13H8a3 3 0 0 1-3-3z M5 17a3 3 0 0 1 3-3h9',
    refresh: 'M20 11a8 8 0 0 0-14.9-3 M4 4v4h4 M4 13a8 8 0 0 0 14.9 3 M20 20v-4h-4',
    file: 'M6 3h8l4 4v14H6z M14 3v4h4',
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
