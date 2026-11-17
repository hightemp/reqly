<!-- "?" next to a feature: opens its help page. -->
<script lang="ts">
  import { t } from '$lib/i18n';
  import IconButton from '$lib/ui/IconButton.svelte';
  import { help } from './help.svelte';

  let { page }: { page: string } = $props();
</script>

<IconButton icon="help" size={14} label={t('help.about')} onclick={() => help.open(page)} />
