<!-- Collection-level scripts: run before the scripts of every folder and request. -->
<script lang="ts">
  import type { Collection } from '$lib/api/generated/Collection';
  import type { Scripts } from '$lib/api/generated/Scripts';
  import { errorMessage } from '$lib/errors';
  import { t } from '$lib/i18n';
  import type { Services } from '$lib/services.svelte';
  import Button from '$lib/ui/Button.svelte';
  import Modal from '$lib/ui/overlay/Modal.svelte';
  import { toasts } from '$lib/ui/overlay/toasts.svelte';
  import ScriptsPanel from './ScriptsPanel.svelte';

  interface Props {
    services: Services;
    collection: Collection;
    onclose: () => void;
  }

  let { services, collection, onclose }: Props = $props();
  // svelte-ignore state_referenced_locally
  let scripts = $state<Scripts>({ ...collection.scripts });
  let saving = $state(false);

  async function save() {
    saving = true;
    try {
      await services.transport.call('collection.updateScripts', { id: collection.id, scripts });
      onclose();
    } catch (e) {
      toasts.error(errorMessage(e));
    } finally {
      saving = false;
    }
  }
</script>

<Modal title={t('scripts.collectionTitle', { name: collection.name })} width="820px" {onclose}>
  <div class="body">
    <ScriptsPanel {scripts} hint={t('scripts.collectionHint')} onchange={(s) => (scripts = s)} />
  </div>
  {#snippet footer()}
    <Button onclick={onclose}>{t('common.cancel')}</Button>
    <Button variant="primary" loading={saving} onclick={() => void save()}
      >{t('scripts.save')}</Button
    >
  {/snippet}
</Modal>

<style>
  .body {
    height: 60vh;
  }
</style>
