<!-- Asks for the values of `{% prompt %}` tags before sending a request. -->
<script lang="ts">
  import { t } from '$lib/i18n';
  import Button from '$lib/ui/Button.svelte';
  import Checkbox from '$lib/ui/Checkbox.svelte';
  import Input from '$lib/ui/Input.svelte';
  import Modal from '$lib/ui/overlay/Modal.svelte';
  import { promptState, type PromptRequest } from './prompts.svelte';

  let { request }: { request: PromptRequest } = $props();
  // svelte-ignore state_referenced_locally
  let values = $state<Record<string, string>>({ ...request.initial });
  let remember = $state(false);

  function submit(e?: SubmitEvent) {
    e?.preventDefault();
    if (remember) promptState.remember(values);
    request.resolve({ ...values });
  }
</script>

<Modal title={t('prompt.title')} onclose={() => request.resolve(null)}>
  <form id="prompt-form" class="form" onsubmit={submit}>
    {#each request.prompts as p (p.key)}
      <label class="field">
        <span class="label">{p.title}{p.label ? ` — ${p.label}` : ''}</span>
        <Input bind:value={values[p.key]} />
      </label>
    {/each}
    <Checkbox bind:checked={remember} showLabel label={t('prompt.remember')} />
  </form>
  {#snippet footer()}
    <Button onclick={() => request.resolve(null)}>{t('common.cancel')}</Button>
    <Button variant="primary" type="submit" form="prompt-form">{t('request.send')}</Button>
  {/snippet}
</Modal>

<style>
  .form {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .label {
    font-size: var(--font-size-sm);
    color: var(--c-fg-muted);
  }
</style>
