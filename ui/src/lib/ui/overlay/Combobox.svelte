<!-- Text input with a filtered suggestion list; optionally accepts custom values. -->
<script lang="ts">
  import Popover from './Popover.svelte';

  interface Props {
    value: string;
    options: string[];
    label: string;
    placeholder?: string;
    allowCustom?: boolean;
    /** Normalizes typed values (e.g. uppercase methods). */
    transform?: (v: string) => string;
    mono?: boolean;
    onchange?: (value: string) => void;
  }

  let {
    value = $bindable(),
    options,
    label,
    placeholder,
    allowCustom = false,
    transform = (v) => v,
    mono = false,
    onchange,
  }: Props = $props();

  let text = $state(value);
  let open = $state(false);
  let active = $state(0);
  let input: HTMLInputElement | null = $state(null);
  const listId = `combo-${Math.random().toString(36).slice(2)}`;

  $effect(() => {
    if (!open) text = value;
  });

  const filtered = $derived(
    text === value
      ? options
      : options.filter((o) => o.toLowerCase().includes(text.trim().toLowerCase())),
  );

  function commit(next: string) {
    const v = transform(next.trim());
    if (!v) {
      text = value;
      return;
    }
    if (!allowCustom && !options.includes(v)) {
      text = value;
      return;
    }
    value = v;
    text = v;
    onchange?.(v);
  }

  function onKeyDown(e: KeyboardEvent) {
    switch (e.key) {
      case 'ArrowDown':
        open = true;
        active = Math.min(active + 1, filtered.length - 1);
        break;
      case 'ArrowUp':
        active = Math.max(active - 1, 0);
        break;
      case 'Enter':
        if (open && filtered[active] !== undefined && text !== value) commit(filtered[active]!);
        else commit(text);
        open = false;
        break;
      case 'Escape':
        if (!open) return;
        text = value;
        open = false;
        break;
      default:
        return;
    }
    e.preventDefault();
    e.stopPropagation();
  }
</script>

<input
  bind:this={input}
  bind:value={text}
  class="combo"
  class:mono
  role="combobox"
  aria-label={label}
  aria-autocomplete="list"
  aria-expanded={open && filtered.length > 0}
  aria-controls={listId}
  aria-activedescendant={open ? `${listId}-${active}` : undefined}
  {placeholder}
  spellcheck="false"
  autocomplete="off"
  onfocus={() => {
    input?.select();
    open = true;
    active = Math.max(0, options.indexOf(value));
  }}
  oninput={() => {
    open = true;
    active = 0;
  }}
  onblur={() => {
    commit(text);
    open = false;
  }}
  onkeydown={onKeyDown}
/>

{#if open && filtered.length > 0 && input}
  <Popover anchor={input} matchWidth ignore={[input]} onclose={() => (open = false)}>
    <div id={listId} class="list" role="listbox" aria-label={label}>
      {#each filtered as opt, i (opt)}
        <div
          id="{listId}-{i}"
          class="option"
          class:active={i === active}
          role="option"
          aria-selected={opt === value}
          tabindex="-1"
          onpointerdown={(e) => {
            e.preventDefault();
            commit(opt);
            open = false;
          }}
          onpointermove={() => (active = i)}
          onkeydown={() => {}}
        >
          {opt}
        </div>
      {/each}
    </div>
  </Popover>
{/if}

<style>
  .combo {
    width: 100%;
    height: var(--control-height);
    padding: 0 var(--space-2);
    border-radius: var(--radius-sm);
    border: 1px solid var(--c-border);
    background: var(--c-bg-sunken);
    color: var(--c-fg);
    outline: none;
  }
  .combo:focus {
    border-color: var(--c-accent);
  }
  .mono {
    font-family: var(--font-mono);
    font-size: var(--font-size-sm);
  }
  .list {
    padding: 4px;
    max-height: 280px;
  }
  .option {
    height: var(--row-height);
    display: flex;
    align-items: center;
    padding: 0 var(--space-2);
    border-radius: var(--radius-sm);
    white-space: nowrap;
  }
  .option.active {
    background: var(--c-bg-hover);
  }
</style>
