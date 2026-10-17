<!-- Renders global overlays: context menu, dialogs, toasts and the tooltip. -->
<script lang="ts">
  import { t } from '$lib/i18n';
  import Button from '../Button.svelte';
  import Input from '../Input.svelte';
  import MenuList from './MenuList.svelte';
  import Modal from './Modal.svelte';
  import Popover from './Popover.svelte';
  import { dialogs, type DialogEntry } from './dialogs.svelte';
  import { menus } from './menus.svelte';
  import { toasts } from './toasts.svelte';
  import { tooltipState } from './tooltip.svelte';

  let promptValue = $state('');
  let promptError = $state<string | null>(null);
  let promptInput: HTMLInputElement | null = $state(null);
  let promptFor = $state<number | null>(null);

  $effect(() => {
    const top = dialogs.stack.at(-1);
    if (top?.kind === 'prompt' && promptFor !== top.id) {
      promptFor = top.id;
      promptValue = top.options.value ?? '';
      promptError = null;
    }
  });

  function submitPrompt(entry: Extract<DialogEntry, { kind: 'prompt' }>) {
    const error = entry.options.validate?.(promptValue) ?? null;
    if (error) {
      promptError = error;
      return;
    }
    entry.resolve(promptValue);
  }
</script>

{#if menus.current}
  {#key menus.current}
    <MenuList
      items={menus.current.items}
      anchor={menus.current.anchor}
      onclose={(restore = true) => menus.close(restore)}
    />
  {/key}
{/if}

{#each dialogs.stack as entry (entry.id)}
  {#if entry.kind === 'confirm'}
    <Modal
      role="alertdialog"
      title={entry.options.title}
      onclose={() => entry.resolve(false)}
      initialFocus={() =>
        document.querySelector<HTMLElement>(`[data-dialog-confirm="${entry.id}"]`)}
    >
      {#if entry.options.message}<p class="message">{entry.options.message}</p>{/if}
      {#snippet footer()}
        <Button onclick={() => entry.resolve(false)}
          >{entry.options.cancelLabel ?? t('common.cancel')}</Button
        >
        <Button
          variant={entry.options.danger ? 'danger' : 'primary'}
          data-dialog-confirm={entry.id}
          onclick={() => entry.resolve(true)}
        >
          {entry.options.confirmLabel ?? t('common.ok')}
        </Button>
      {/snippet}
    </Modal>
  {:else}
    <Modal
      title={entry.options.title}
      onclose={() => entry.resolve(null)}
      initialFocus={() => promptInput}
    >
      <label class="field">
        <span class="label">{entry.options.label}</span>
        <Input
          bind:inputRef={promptInput}
          bind:value={promptValue}
          aria-label={entry.options.label}
          placeholder={entry.options.placeholder}
          invalid={promptError !== null}
          onfocus={(e) => {
            const input = e.currentTarget as HTMLInputElement;
            input.setSelectionRange(0, entry.options.selectUntil ?? input.value.length);
          }}
          oninput={() => (promptError = null)}
          onkeydown={(e) => {
            if (e.key === 'Enter') {
              e.preventDefault();
              submitPrompt(entry);
            }
          }}
        />
        {#if promptError}<span class="error" role="alert">{promptError}</span>{/if}
      </label>
      {#snippet footer()}
        <Button onclick={() => entry.resolve(null)}>{t('common.cancel')}</Button>
        <Button variant="primary" onclick={() => submitPrompt(entry)}
          >{entry.options.confirmLabel ?? t('common.ok')}</Button
        >
      {/snippet}
    </Modal>
  {/if}
{/each}

<div class="toasts" aria-live="polite">
  {#each toasts.items as toast (toast.id)}
    <div class="toast {toast.kind}" role="status">
      <span class="toast-message">{toast.message}</span>
      {#if toast.action}
        {@const action = toast.action}
        <button
          type="button"
          class="toast-action"
          onclick={() => {
            action.run();
            toasts.dismiss(toast.id);
          }}>{action.label}</button
        >
      {/if}
      <button
        type="button"
        class="toast-close"
        aria-label={t('common.close')}
        onclick={() => toasts.dismiss(toast.id)}>×</button
      >
    </div>
  {/each}
</div>

{#if tooltipState.text && tooltipState.anchor}
  <Popover
    anchor={tooltipState.anchor}
    placement="bottom"
    offset={6}
    role="tooltip"
    class="tooltip"
    onclose={() => (tooltipState.text = null)}
  >
    {tooltipState.text}
  </Popover>
{/if}

<style>
  .message {
    color: var(--c-fg-muted);
    white-space: pre-wrap;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }
  .label {
    font-size: var(--font-size-sm);
    color: var(--c-fg-muted);
  }
  .error {
    color: var(--c-danger);
    font-size: var(--font-size-sm);
  }
  .toasts {
    position: fixed;
    right: var(--space-4);
    bottom: calc(var(--statusbar-height) + var(--space-3));
    z-index: 950;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    pointer-events: none;
  }
  .toast {
    pointer-events: auto;
    display: flex;
    align-items: center;
    gap: var(--space-3);
    min-width: 260px;
    max-width: 420px;
    padding: var(--space-2) var(--space-2) var(--space-2) var(--space-3);
    border-radius: var(--radius);
    background: var(--c-bg-elevated);
    border: 1px solid var(--c-border-strong);
    border-left: 3px solid var(--c-info);
    box-shadow: 0 8px 24px var(--c-shadow);
  }
  .toast.success {
    border-left-color: var(--c-success);
  }
  .toast.warning {
    border-left-color: var(--c-warning);
  }
  .toast.error {
    border-left-color: var(--c-danger);
  }
  .toast-message {
    flex: 1;
    user-select: text;
  }
  .toast-action {
    color: var(--c-accent);
    font-weight: 600;
  }
  .toast-close {
    width: 20px;
    height: 20px;
    border-radius: var(--radius-sm);
    color: var(--c-fg-subtle);
  }
  .toast-close:hover {
    background: var(--c-bg-hover);
  }
  :global(.tooltip) {
    padding: 4px 8px;
    font-size: var(--font-size-xs);
    pointer-events: none;
    max-width: 320px;
  }
</style>
