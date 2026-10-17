<!-- Template options: faker data locale, globally and for the active collection. -->
<script lang="ts">
  import { onMount } from 'svelte';
  import { t } from '$lib/i18n';
  import type { Services } from '$lib/services.svelte';
  import Select from '$lib/ui/overlay/Select.svelte';
  import Field from '../Field.svelte';

  let { services }: { services: Services } = $props();
  const KEY = 'template.fakerLocale';
  const collectionId = $derived(services.workspace.activeCollectionId);
  const collectionKey = $derived(collectionId ? `${KEY}.${collectionId}` : null);

  let global = $state('en');
  let collection = $state('');

  onMount(async () => {
    global =
      ((await services.transport.call('settings.get', { key: KEY })).value as string) ?? 'en';
    if (collectionKey)
      collection =
        ((await services.transport.call('settings.get', { key: collectionKey })).value as string) ??
        '';
  });

  const locales = $derived([
    { value: 'en', label: t('languages.en') },
    { value: 'ru', label: t('languages.ru') },
  ]);

  async function setGlobal(value: string) {
    global = value;
    await services.transport.call('settings.set', { key: KEY, value });
  }

  async function setCollection(value: string) {
    if (!collectionKey) return;
    collection = value;
    await services.transport.call('settings.set', {
      key: collectionKey,
      value: value === '' ? null : value,
    });
  }
</script>

<p class="hint">{t('settings.templatesHint')}</p>
<Field label={t('settings.fakerLocale')}>
  <Select label={t('settings.fakerLocale')} value={global} options={locales} onchange={setGlobal} />
</Field>
{#if collectionKey}
  <Field label={t('settings.fakerLocaleCollection')}>
    <Select
      label={t('settings.fakerLocaleCollection')}
      value={collection}
      options={[{ value: '', label: t('settings.sameAsGlobal') }, ...locales]}
      onchange={setCollection}
    />
  </Field>
{/if}

<style>
  .hint {
    color: var(--c-fg-subtle);
    margin-bottom: var(--space-3);
  }
</style>
