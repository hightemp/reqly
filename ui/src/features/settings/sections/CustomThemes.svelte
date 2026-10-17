<!-- Custom themes: create from an existing one, edit tokens with live preview, import/export JSON. -->
<script lang="ts">
  import { errorMessage } from '$lib/errors';
  import { t } from '$lib/i18n';
  import { browserChoose } from '$lib/platform/files';
  import { CUSTOM_THEMES_KEY, type Services } from '$lib/services.svelte';
  import { isColor, parseTheme, type Theme } from '$lib/themes/theme';
  import { COLOR_TOKENS, type ColorToken } from '$lib/themes/tokens';
  import Button from '$lib/ui/Button.svelte';
  import EmptyState from '$lib/ui/EmptyState.svelte';
  import Input from '$lib/ui/Input.svelte';
  import { dialogs } from '$lib/ui/overlay/dialogs.svelte';
  import Select from '$lib/ui/overlay/Select.svelte';
  import { toasts } from '$lib/ui/overlay/toasts.svelte';

  let { services }: { services: Services } = $props();
  const themes = $derived(services.themes);
  let editing = $state<string | null>(null);
  const current = $derived(themes.custom.find((th) => th.id === editing) ?? null);
  const resolved = $derived(themes.themes.find((th) => th.id === editing) ?? null);

  async function persist() {
    await services.transport.call('settings.set', { key: CUSTOM_THEMES_KEY, value: themes.custom });
  }

  async function create(baseId: string) {
    const base = themes.themes.find((th) => th.id === baseId);
    const name = await dialogs.prompt({
      title: t('themes.newTitle'),
      label: t('actions.name'),
      value: `${base?.name ?? ''} ${t('themes.copySuffix')}`.trim(),
    });
    if (!name) return;
    const id = `custom-${crypto.randomUUID().slice(0, 8)}`;
    themes.addCustom({ id, name, kind: base?.kind ?? 'dark', extends: baseId, colors: {} });
    editing = id;
    await persist();
  }

  async function setColor(token: ColorToken, value: string) {
    if (!current || !isColor(value)) return;
    themes.addCustom({ ...current, colors: { ...current.colors, [token]: value } });
    themes.apply();
    await persist();
  }

  async function remove(id: string) {
    const ok = await dialogs.confirm({
      title: t('themes.deleteTitle'),
      danger: true,
      confirmLabel: t('tree.delete'),
    });
    if (!ok) return;
    if (services.prefs.value.theme === id) await services.prefs.update({ theme: 'system' });
    themes.removeCustom(id);
    editing = null;
    await persist();
  }

  async function importTheme() {
    const file = await browserChoose();
    if (!file) return;
    try {
      const parsed = parseTheme(JSON.parse(await file.text()));
      if (!parsed.ok) throw new Error(parsed.errors.slice(0, 3).join('; '));
      const theme: Theme = {
        ...parsed.theme,
        id: themes.themes.some((th) => th.id === parsed.theme.id)
          ? `custom-${crypto.randomUUID().slice(0, 8)}`
          : parsed.theme.id,
      };
      themes.addCustom(theme);
      await persist();
      toasts.show({ message: t('themes.imported', { name: theme.name }), kind: 'success' });
    } catch (e) {
      toasts.error(t('themes.importFailed', { reason: errorMessage(e) }));
    }
  }

  function exportTheme(id: string) {
    const th = themes.themes.find((x) => x.id === id);
    if (!th) return;
    const blob = new Blob(
      [JSON.stringify({ id: th.id, name: th.name, kind: th.kind, colors: th.colors }, null, 2)],
      { type: 'application/json' },
    );
    const a = document.createElement('a');
    a.href = URL.createObjectURL(blob);
    a.download = `${th.id}.reqly-theme.json`;
    a.click();
    setTimeout(() => URL.revokeObjectURL(a.href), 1000);
  }

  const toHex = (c: string) => (/^#[0-9a-f]{6}$/i.test(c) ? c : '#000000');
</script>

<div class="toolbar">
  <Select
    label={t('themes.createFrom')}
    value={'' as string}
    options={[
      { value: '', label: t('themes.createFrom') },
      ...themes.themes.map((th) => ({ value: th.id, label: th.name })),
    ]}
    onchange={(id) => id && void create(id)}
  />
  <Button onclick={() => void importTheme()}>{t('themes.import')}</Button>
</div>

{#if themes.custom.length === 0}
  <EmptyState title={t('themes.none')} />
{:else}
  <div class="list">
    {#each themes.custom as th (th.id)}
      <div class="item" class:active={editing === th.id}>
        <button
          type="button"
          class="pick"
          onclick={() => (editing = editing === th.id ? null : th.id)}>{th.name}</button
        >
        <Button
          size="sm"
          variant="ghost"
          onclick={() => void services.prefs.update({ theme: th.id })}>{t('themes.use')}</Button
        >
        <Button size="sm" variant="ghost" onclick={() => exportTheme(th.id)}
          >{t('themes.export')}</Button
        >
        <Button size="sm" variant="ghost" onclick={() => void remove(th.id)}
          >{t('tree.delete')}</Button
        >
      </div>
    {/each}
  </div>
{/if}

{#if current && resolved}
  <div class="editor">
    {#each COLOR_TOKENS as token (token)}
      <label class="token">
        <input
          type="color"
          value={toHex(resolved.colors[token])}
          aria-label={token}
          onchange={(e) => void setColor(token, e.currentTarget.value)}
        />
        <span class="token-name">{token}</span>
        <Input
          mono
          value={resolved.colors[token]}
          aria-label={token}
          onchange={(e) => void setColor(token, e.currentTarget.value.trim())}
        />
      </label>
    {/each}
  </div>
{/if}

<style>
  .toolbar {
    display: flex;
    gap: var(--space-2);
    margin-bottom: var(--space-3);
  }
  .list {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .item {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    padding: 2px var(--space-2);
    border-radius: var(--radius-sm);
  }
  .item.active {
    background: var(--c-bg-selected);
  }
  .pick {
    flex: 1;
    text-align: left;
    padding: 4px 0;
  }
  .editor {
    margin-top: var(--space-3);
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
    gap: var(--space-1) var(--space-3);
  }
  .token {
    display: grid;
    grid-template-columns: 28px 1fr 140px;
    align-items: center;
    gap: var(--space-2);
  }
  .token input[type='color'] {
    width: 28px;
    height: 22px;
    padding: 0;
    border: 1px solid var(--c-border);
    border-radius: 4px;
    background: none;
  }
  .token-name {
    font-family: var(--font-mono);
    font-size: var(--font-size-xs);
    color: var(--c-fg-muted);
    overflow: hidden;
    text-overflow: ellipsis;
  }
</style>
