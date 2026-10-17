<script lang="ts">
  import { LOCALES, i18n, t } from '$lib/i18n';
  import type { Services } from '$lib/services.svelte';
  import Select from '$lib/ui/overlay/Select.svelte';
  import Switch from '$lib/ui/Switch.svelte';
  import Field from '../Field.svelte';

  let { services }: { services: Services } = $props();
  const prefs = $derived(services.prefs);

  // i18n-dynamic: languages.
  const languages = $derived([
    { value: 'system', label: t('settings.languageSystem') },
    ...LOCALES.map((l) => ({ value: l, label: t(`languages.${l}`) })),
  ]);
</script>

<Field label={t('settings.language')}>
  <Select
    label={t('settings.language')}
    value={prefs.value.locale ?? 'system'}
    options={languages}
    onchange={(v) => {
      const locale = v === 'system' ? null : (v as (typeof LOCALES)[number]);
      if (locale) i18n.setLocale(locale);
      void prefs.update({ locale });
    }}
  />
</Field>
<Field label={t('settings.autosave')} hint={t('settings.autosaveHint')}>
  <Switch
    label={t('settings.autosave')}
    checked={prefs.value.autosave}
    onchange={(v) => void prefs.update({ autosave: v })}
  />
</Field>
<Field label={t('settings.sidebarMode')}>
  <Select
    label={t('settings.sidebarMode')}
    value={prefs.value.sidebarMode}
    options={[
      { value: 'tree', label: t('sidebar.modeTree') },
      { value: 'categories', label: t('sidebar.modeCategories') },
    ]}
    onchange={(v) => void prefs.update({ sidebarMode: v })}
  />
</Field>
<Field label={t('settings.flatList')} hint={t('settings.flatListHint')}>
  <Switch
    label={t('settings.flatList')}
    checked={prefs.value.flatList}
    onchange={(v) => void prefs.update({ flatList: v })}
  />
</Field>
<Field label={t('settings.responseLayout')}>
  <Select
    label={t('settings.responseLayout')}
    value={prefs.value.responseLayout}
    options={[
      { value: 'right', label: t('settings.layoutRight') },
      { value: 'bottom', label: t('settings.layoutBottom') },
    ]}
    onchange={(v) => void prefs.update({ responseLayout: v })}
  />
</Field>
