<!-- Custom select: a combobox button with a listbox popover. -->
<script lang="ts" module>
  export interface SelectOption<V extends string = string> {
    value: V;
    label: string;
    hint?: string;
    disabled?: boolean;
  }
</script>

<script lang="ts" generics="T extends string">
  import Popover from './Popover.svelte';

  interface Props {
    value: T;
    options: SelectOption<T>[];
    label: string;
    disabled?: boolean;
    compact?: boolean;
    onchange?: (value: T) => void;
  }

  let {
    value = $bindable(),
    options,
    label,
    disabled = false,
    compact = false,
    onchange,
  }: Props = $props();

  let open = $state(false);
  let active = $state(0);
  let trigger: HTMLButtonElement | null = $state(null);
  let list: HTMLDivElement | null = $state(null);
  const listId = `select-${Math.random().toString(36).slice(2)}`;
  const current = $derived(options.find((o) => o.value === value));

  function show() {
    if (disabled) return;
    active = Math.max(
      0,
      options.findIndex((o) => o.value === value),
    );
    open = true;
    queueMicrotask(() => list?.focus());
  }

  function close(focus = true) {
    open = false;
    if (focus) trigger?.focus();
  }

  function choose(i: number) {
    const opt = options[i];
    if (!opt || opt.disabled) return;
    value = opt.value;
    onchange?.(opt.value);
    close();
  }

  function move(delta: number) {
    if (options.length === 0) return;
    let i = active;
    for (let n = 0; n < options.length; n++) {
      i = (i + delta + options.length) % options.length;
      if (!options[i]!.disabled) break;
    }
    active = i;
    list?.querySelector(`#${listId}-${i}`)?.scrollIntoView?.({ block: 'nearest' });
  }

  function typeahead(key: string): number {
    const start = (open ? active : options.findIndex((o) => o.value === value)) + 1;
    for (let n = 0; n < options.length; n++) {
      const i = (start + n) % options.length;
      if (options[i]!.label.toLowerCase().startsWith(key.toLowerCase()) && !options[i]!.disabled)
        return i;
    }
    return -1;
  }

  function onTriggerKey(e: KeyboardEvent) {
    if (['ArrowDown', 'ArrowUp', 'Enter', ' '].includes(e.key)) {
      e.preventDefault();
      show();
    } else if (e.key.length === 1 && /\S/.test(e.key)) {
      const i = typeahead(e.key);
      if (i >= 0) {
        value = options[i]!.value;
        onchange?.(value);
      }
    }
  }

  function onListKey(e: KeyboardEvent) {
    switch (e.key) {
      case 'ArrowDown':
        move(1);
        break;
      case 'ArrowUp':
        move(-1);
        break;
      case 'Home':
        active = 0;
        break;
      case 'End':
        active = options.length - 1;
        break;
      case 'Enter':
      case ' ':
        choose(active);
        break;
      case 'Tab':
        close(false);
        return;
      default:
        if (e.key.length === 1) {
          const i = typeahead(e.key);
          if (i >= 0) active = i;
          break;
        }
        return;
    }
    e.preventDefault();
  }
</script>

<button
  bind:this={trigger}
  type="button"
  class="select"
  class:compact
  role="combobox"
  aria-label={label}
  aria-haspopup="listbox"
  aria-expanded={open}
  aria-controls={open ? listId : undefined}
  {disabled}
  onclick={() => (open ? close() : show())}
  onkeydown={onTriggerKey}
>
  <span class="value">{current?.label ?? value}</span>
  <svg class="chevron" viewBox="0 0 16 16" width="12" height="12" aria-hidden="true"
    ><path
      d="M4 6l4 4 4-4"
      fill="none"
      stroke="currentColor"
      stroke-width="1.6"
      stroke-linecap="round"
      stroke-linejoin="round"
    /></svg
  >
</button>

{#if open && trigger}
  <Popover anchor={trigger} matchWidth ignore={[trigger]} onclose={(r) => close(r === 'escape')}>
    <div
      bind:this={list}
      id={listId}
      class="list"
      role="listbox"
      tabindex="-1"
      aria-label={label}
      aria-activedescendant="{listId}-{active}"
      onkeydown={onListKey}
    >
      {#each options as opt, i (opt.value)}
        <div
          id="{listId}-{i}"
          class="option"
          class:active={i === active}
          role="option"
          aria-selected={opt.value === value}
          aria-disabled={opt.disabled || undefined}
          tabindex="-1"
          onpointermove={() => (active = i)}
          onclick={() => choose(i)}
          onkeydown={() => {}}
        >
          <span>{opt.label}</span>
          {#if opt.hint}<span class="hint">{opt.hint}</span>{/if}
        </div>
      {/each}
    </div>
  </Popover>
{/if}

<style>
  .select {
    display: inline-flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
    min-width: 0;
    height: var(--control-height);
    padding: 0 var(--space-2);
    border-radius: var(--radius-sm);
    border: 1px solid var(--c-border);
    background: var(--c-bg-sunken);
    color: var(--c-fg);
    text-align: left;
  }
  .select:hover {
    border-color: var(--c-border-strong);
  }
  .select[aria-expanded='true'] {
    border-color: var(--c-accent);
  }
  .compact {
    border-color: transparent;
    background: transparent;
  }
  .value {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .chevron {
    flex: none;
    color: var(--c-fg-subtle);
  }
  .list {
    padding: 4px;
    outline: none;
  }
  .option {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    height: var(--row-height);
    padding: 0 var(--space-2);
    border-radius: var(--radius-sm);
    white-space: nowrap;
  }
  .option.active {
    background: var(--c-bg-hover);
  }
  .option[aria-selected='true'] {
    color: var(--c-accent);
    font-weight: 600;
  }
  .option[aria-disabled='true'] {
    opacity: 0.45;
  }
  .hint {
    color: var(--c-fg-subtle);
    font-size: var(--font-size-xs);
  }
</style>
