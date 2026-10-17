<script lang="ts">
  import { isTauri } from '$lib/api/transport';
  import { t } from '$lib/i18n';
  import type { Services } from '$lib/services.svelte';
  import type { ResolvedTheme } from '$lib/themes/theme';
  import Input from '$lib/ui/Input.svelte';
  import Select from '$lib/ui/overlay/Select.svelte';
  import Switch from '$lib/ui/Switch.svelte';
  import Field from '../Field.svelte';

  let { services }: { services: Services } = $props();
  const prefs = $derived(services.prefs);
  const themes = $derived(services.themes);

  const sizes = (from: number, to: number) =>
    Array.from({ length: to - from + 1 }, (_, i) => ({
      value: String(from + i),
      label: `${from + i}px`,
    }));
  const zooms = [80, 90, 100, 110, 125, 150].map((z) => ({ value: String(z), label: `${z}%` }));

  const swatches = (th: ResolvedTheme) => [
    th.colors['method-get'],
    th.colors['method-post'],
    th.colors['method-put'],
    th.colors['method-delete'],
    th.colors.accent,
  ];
</script>

<h3 class="title">{t('settings.theme')}</h3>
<div class="themes" role="radiogroup" aria-label={t('settings.theme')}>
  <button
    type="button"
    role="radio"
    aria-checked={prefs.value.theme === 'system'}
    class="theme system"
    onclick={() => void prefs.update({ theme: 'system' })}
  >
    <span
      class="preview split"
      style:--a={themes.themes[0]!.colors.bg}
      style:--b={themes.themes[1]!.colors.bg}
    ></span>
    <span class="name">{t('settings.themeSystem')}</span>
  </button>
  {#each themes.themes as th (th.id)}
    <button
      type="button"
      role="radio"
      aria-checked={prefs.value.theme === th.id}
      class="theme"
      onclick={() => void prefs.update({ theme: th.id })}
    >
      <span class="preview" style:background={th.colors.bg}>
        <span class="side" style:background={th.colors['bg-elevated']}></span>
        <span class="bar" style:background={th.colors.accent}></span>
        <span class="dots">
          {#each swatches(th) as c, i (i)}<span style:background={c}></span>{/each}
        </span>
      </span>
      <span class="name">{th.name}</span>
    </button>
  {/each}
</div>

<Field label={t('settings.density')}>
  <Select
    label={t('settings.density')}
    value={prefs.value.density}
    options={[
      { value: 'comfortable', label: t('settings.densityComfortable') },
      { value: 'compact', label: t('settings.densityCompact') },
    ]}
    onchange={(v) => void prefs.update({ density: v })}
  />
</Field>
<Field label={t('settings.zoom')}>
  <Select
    label={t('settings.zoom')}
    value={String(prefs.value.zoom)}
    options={zooms}
    onchange={(v) => void prefs.update({ zoom: Number(v) })}
  />
</Field>
<Field label={t('settings.uiFontSize')}>
  <Select
    label={t('settings.uiFontSize')}
    value={String(prefs.value.uiFontSize)}
    options={sizes(11, 16)}
    onchange={(v) => void prefs.update({ uiFontSize: Number(v) })}
  />
</Field>
<Field label={t('settings.uiFont')} hint={t('settings.fontHint')}>
  <Input
    value={prefs.value.fontUi}
    aria-label={t('settings.uiFont')}
    onchange={(e) => void prefs.update({ fontUi: e.currentTarget.value })}
  />
</Field>
<Field label={t('settings.editorFontSize')}>
  <Select
    label={t('settings.editorFontSize')}
    value={String(prefs.value.editorFontSize)}
    options={sizes(10, 18)}
    onchange={(v) => void prefs.update({ editorFontSize: Number(v) })}
  />
</Field>
<Field label={t('settings.editorFont')} hint={t('settings.fontHint')}>
  <Input
    mono
    value={prefs.value.fontMono}
    aria-label={t('settings.editorFont')}
    onchange={(e) => void prefs.update({ fontMono: e.currentTarget.value })}
  />
</Field>
<Field label={t('settings.editorWrap')}>
  <Switch
    label={t('settings.editorWrap')}
    checked={prefs.value.editorWrap}
    onchange={(v) => void prefs.update({ editorWrap: v })}
  />
</Field>
{#if isTauri()}
  <Field label={t('settings.nativeFrame')} hint={t('settings.restartHint')}>
    <Switch
      label={t('settings.nativeFrame')}
      checked={prefs.value.nativeFrame}
      onchange={(v) => void prefs.update({ nativeFrame: v })}
    />
  </Field>
{/if}

<style>
  .title {
    font-weight: 600;
    margin-bottom: var(--space-2);
  }
  .themes {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(118px, 1fr));
    gap: var(--space-3);
    margin-bottom: var(--space-4);
  }
  .theme {
    display: flex;
    flex-direction: column;
    gap: 6px;
    align-items: center;
    padding: 6px;
    border-radius: var(--radius);
    border: 2px solid transparent;
  }
  .theme:hover {
    background: var(--c-bg-hover);
  }
  .theme[aria-checked='true'] {
    border-color: var(--c-accent);
  }
  .preview {
    position: relative;
    width: 100%;
    height: 64px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--c-border);
    overflow: hidden;
  }
  .preview.split {
    background: linear-gradient(135deg, var(--a) 50%, var(--b) 50%);
  }
  .side {
    position: absolute;
    inset: 0 auto 0 0;
    width: 28%;
  }
  .bar {
    position: absolute;
    top: 8px;
    right: 8px;
    width: 30%;
    height: 6px;
    border-radius: 3px;
  }
  .dots {
    position: absolute;
    left: 34%;
    bottom: 8px;
    display: flex;
    gap: 4px;
  }
  .dots span {
    width: 8px;
    height: 8px;
    border-radius: 2px;
  }
  .name {
    font-size: var(--font-size-sm);
  }
</style>
