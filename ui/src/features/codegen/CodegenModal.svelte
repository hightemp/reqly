<!-- "Generate code": the active request as a snippet for a chosen language and library. -->
<script lang="ts">
  import type { Target } from '$lib/api/generated/Target';
  import type { TargetInfo } from '$lib/api/generated/TargetInfo';
  import { errorMessage } from '$lib/errors';
  import { t } from '$lib/i18n';
  import type { Services } from '$lib/services.svelte';
  import Button from '$lib/ui/Button.svelte';
  import Checkbox from '$lib/ui/Checkbox.svelte';
  import CodeEditor, { type CodeLanguage } from '$lib/ui/code/CodeEditor.svelte';
  import Modal from '$lib/ui/overlay/Modal.svelte';
  import Select from '$lib/ui/overlay/Select.svelte';
  import { toasts } from '$lib/ui/overlay/toasts.svelte';

  interface Props {
    services: Services;
    requestId: string;
    onclose: () => void;
  }

  let { services, requestId, onclose }: Props = $props();
  const TARGET_KEY = 'ui.codegenTarget';

  let targets = $state<TargetInfo[]>([]);
  let target = $state<Target>('curlBash');
  let resolve = $state(true);
  let code = $state('');
  let error = $state<string | null>(null);

  $effect(() => {
    void Promise.all([
      services.transport.call('codegen.targets', {}),
      services.transport.call('settings.get', { key: TARGET_KEY }),
    ]).then(([list, saved]) => {
      targets = list;
      if (typeof saved.value === 'string' && list.some((x) => x.id === saved.value))
        target = saved.value as Target;
    });
  });

  $effect(() => {
    const current = { requestId, target, resolve };
    void services.editor.flush(requestId).then(() =>
      services.transport.call('codegen.generate', current).then(
        (out) => {
          code = out.code;
          error = null;
        },
        (e: unknown) => {
          code = '';
          error = errorMessage(e);
        },
      ),
    );
  });

  const info = $derived(targets.find((x) => x.id === target));
  const LANGS: Record<string, CodeLanguage> = { javascript: 'javascript' };
  const language = $derived<CodeLanguage>(LANGS[info?.syntax ?? ''] ?? 'text');

  function choose(next: Target) {
    target = next;
    void services.transport.call('settings.set', { key: TARGET_KEY, value: next });
  }

  async function copy() {
    await navigator.clipboard.writeText(code);
    toasts.show({ message: t('codegen.copied'), kind: 'success' });
  }
</script>

<Modal title={t('codegen.title')} width="820px" {onclose}>
  <div class="toolbar">
    <div class="target">
      <Select
        label={t('codegen.target')}
        value={target}
        options={targets.map((x) => ({ value: x.id, label: `${x.language} — ${x.label}` }))}
        onchange={choose}
      />
    </div>
    <Checkbox bind:checked={resolve} showLabel label={t('codegen.resolve')} />
  </div>
  {#if error}
    <p class="error" role="alert">{error}</p>
  {/if}
  <div class="code">
    <CodeEditor value={code} {language} readonly label={t('codegen.code')} />
  </div>
  {#snippet footer()}
    <Button onclick={onclose}>{t('common.close')}</Button>
    <Button variant="primary" disabled={!code} onclick={() => void copy()}
      >{t('codegen.copy')}</Button
    >
  {/snippet}
</Modal>

<style>
  .toolbar {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    margin-bottom: var(--space-2);
  }
  .target {
    width: 320px;
  }
  .code {
    height: min(460px, 60vh);
    border: 1px solid var(--c-border);
    border-radius: var(--radius-sm);
    background: var(--c-bg-sunken);
  }
  .error {
    margin: 0 0 var(--space-2);
    color: var(--c-danger);
  }
</style>
