<!-- Shortcut list with recording, reset and conflict detection. -->
<script lang="ts">
  import { t } from '$lib/i18n';
  import { comboFromEvent, formatCombo } from '$lib/hotkeys/hotkeys';
  import { KEYBINDINGS_KEY, type Services } from '$lib/services.svelte';
  import Button from '$lib/ui/Button.svelte';
  import Kbd from '$lib/ui/Kbd.svelte';

  let { services, filter = '' }: { services: Services; filter?: string } = $props();
  const hotkeys = $derived(services.hotkeys);
  let recording = $state<string | null>(null);
  let version = $state(0);

  // i18n-dynamic: hotkeys.
  function label(id: string) {
    const goto = /^tab\.goto(\d)$/.exec(id);
    return goto ? t('hotkeys.tabGoto', { n: goto[1]! }) : t(`hotkeys.${id}` as never);
  }

  const rows = $derived.by(() => {
    void version;
    const q = filter.toLowerCase();
    return hotkeys
      .definitions()
      .map((a) => ({
        id: a.id,
        label: label(a.id),
        combos: hotkeys.combosFor(a.id),
        overridden: a.id in hotkeys.getOverrides(),
      }))
      .filter((r) => !q || r.label.toLowerCase().includes(q) || r.id.includes(q));
  });
  const conflicts = $derived.by(() => {
    void version;
    return new Set(hotkeys.conflicts().flatMap((c) => c.actions));
  });

  async function persist() {
    version++;
    await services.transport.call('settings.set', {
      key: KEYBINDINGS_KEY,
      value: hotkeys.getOverrides(),
    });
  }

  function onRecordKey(e: KeyboardEvent, id: string) {
    e.preventDefault();
    e.stopPropagation();
    if (e.key === 'Escape') {
      recording = null;
      return;
    }
    const combo = comboFromEvent(e, services.platform);
    if (!combo) return;
    hotkeys.setOverride(id, [combo]);
    recording = null;
    void persist();
  }
</script>

<table class="keys">
  <tbody>
    {#each rows as row (row.id)}
      <tr class:conflict={conflicts.has(row.id)}>
        <td class="name">{row.label}</td>
        <td class="combo">
          {#if recording === row.id}
            <!-- svelte-ignore a11y_autofocus -->
            <input
              class="record"
              readonly
              autofocus
              aria-label={t('hotkeys.press')}
              placeholder={t('hotkeys.press')}
              onkeydown={(e) => onRecordKey(e, row.id)}
              onblur={() => (recording = null)}
            />
          {:else}
            {#each row.combos as c (c)}<Kbd keys={formatCombo(c, services.platform)} />{/each}
          {/if}
          {#if conflicts.has(row.id)}<span class="warn">{t('hotkeys.conflict')}</span>{/if}
        </td>
        <td class="actions">
          <Button size="sm" variant="ghost" onclick={() => (recording = row.id)}
            >{t('hotkeys.change')}</Button
          >
          {#if row.overridden}
            <Button
              size="sm"
              variant="ghost"
              onclick={() => {
                hotkeys.resetOverride(row.id);
                void persist();
              }}>{t('hotkeys.reset')}</Button
            >
          {/if}
        </td>
      </tr>
    {/each}
  </tbody>
</table>

<style>
  .keys {
    width: 100%;
    border-collapse: collapse;
  }
  td {
    padding: 4px var(--space-2);
    border-bottom: 1px solid var(--c-border);
  }
  .combo {
    display: flex;
    gap: 4px;
    align-items: center;
    min-height: 32px;
  }
  .actions {
    text-align: right;
    white-space: nowrap;
  }
  .conflict .name {
    color: var(--c-warning);
  }
  .warn {
    font-size: var(--font-size-xs);
    color: var(--c-warning);
  }
  .record {
    width: 160px;
    height: 24px;
    padding: 0 var(--space-2);
    border: 1px solid var(--c-accent);
    border-radius: var(--radius-sm);
    background: var(--c-bg-sunken);
    color: var(--c-fg);
    outline: none;
  }
</style>
