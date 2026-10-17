<!--
  Custom window frame for the desktop app. On macOS the native traffic lights
  are kept (overlay title bar), so only the drag region is drawn.
-->
<script lang="ts">
  import { onMount, type Snippet } from 'svelte';
  import { t } from '$lib/i18n';
  import type { Platform } from '$lib/hotkeys/hotkeys';
  import type { WindowController } from '$lib/platform/window';
  import IconButton from '$lib/ui/IconButton.svelte';

  interface Props {
    controller: WindowController;
    platform: Platform;
    children?: Snippet;
  }

  let { controller, platform, children }: Props = $props();
  let maximized = $state(false);

  const sync = async () => {
    maximized = await controller.isMaximized();
  };

  onMount(() => {
    void sync();
    const off = controller.onResized(() => void sync());
    return () => void off.then((fn) => fn());
  });
</script>

<header class="titlebar" class:mac={platform === 'mac'}>
  <div
    class="drag"
    data-testid="titlebar-drag"
    data-tauri-drag-region
    role="presentation"
    ondblclick={() => controller.toggleMaximize()}
  >
    <span class="name" data-tauri-drag-region>{t('app.name')}</span>
    {@render children?.()}
  </div>
  {#if platform !== 'mac'}
    <div class="controls">
      <IconButton
        icon="minimize"
        label={t('window.minimize')}
        onclick={() => controller.minimize()}
      />
      <IconButton
        icon={maximized ? 'restore' : 'maximize'}
        label={maximized ? t('window.restore') : t('window.maximize')}
        onclick={() => controller.toggleMaximize()}
      />
      <IconButton
        icon="close"
        variant="danger"
        label={t('window.close')}
        onclick={() => controller.close()}
      />
    </div>
  {/if}
</header>

<style>
  .titlebar {
    display: flex;
    align-items: stretch;
    height: var(--titlebar-height);
    flex: none;
    background: var(--c-bg-elevated);
    border-bottom: 1px solid var(--c-border);
  }
  .drag {
    flex: 1;
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: 0 var(--space-3);
    min-width: 0;
  }
  /* Room for the macOS traffic lights. */
  .mac .drag {
    padding-left: 78px;
  }
  .name {
    font-weight: 600;
    color: var(--c-fg-muted);
    letter-spacing: 0.02em;
  }
  .controls {
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 0 var(--space-1);
  }
</style>
