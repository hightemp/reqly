<script lang="ts">
  import { onMount } from 'svelte';
  import type { HistoryLimitSettings } from '$lib/api/generated/HistoryLimitSettings';
  import { t } from '$lib/i18n';
  import type { Services } from '$lib/services.svelte';
  import Input from '$lib/ui/Input.svelte';
  import Field from '../Field.svelte';

  let { services }: { services: Services } = $props();
  const KEY = 'history.limits';
  let limits = $state<HistoryLimitSettings>({
    perRequest: 20,
    totalMegabytes: 1024,
    maxAgeDays: null,
  });

  onMount(async () => {
    const { value } = await services.transport.call('settings.get', { key: KEY });
    if (value) limits = { ...limits, ...(value as Partial<HistoryLimitSettings>) };
  });

  async function update(patch: Partial<HistoryLimitSettings>) {
    limits = { ...limits, ...patch };
    await services.transport.call('settings.set', { key: KEY, value: limits });
  }

  const num = (v: string, min: number) => Math.max(min, Math.floor(Number(v) || min));
</script>

<Field label={t('settings.historyPerRequest')}>
  <Input
    type="number"
    min="1"
    value={String(limits.perRequest)}
    aria-label={t('settings.historyPerRequest')}
    onchange={(e) => void update({ perRequest: num(e.currentTarget.value, 1) })}
  />
</Field>
<Field label={t('settings.historyTotal')} hint={t('settings.historyTotalHint')}>
  <Input
    type="number"
    min="1"
    value={String(limits.totalMegabytes)}
    aria-label={t('settings.historyTotal')}
    onchange={(e) => void update({ totalMegabytes: num(e.currentTarget.value, 1) })}
  />
</Field>
<Field label={t('settings.historyAge')} hint={t('settings.historyAgeHint')}>
  <Input
    type="number"
    min="1"
    value={limits.maxAgeDays == null ? '' : String(limits.maxAgeDays)}
    placeholder={t('settings.forever')}
    aria-label={t('settings.historyAge')}
    onchange={(e) =>
      void update({
        maxAgeDays: e.currentTarget.value.trim() ? num(e.currentTarget.value, 1) : null,
      })}
  />
</Field>
