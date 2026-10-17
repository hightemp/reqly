<!-- Response side of a connection: status, message log with filters, details and handshake headers. -->
<script lang="ts">
  import type { StreamMessage } from '$lib/api/generated/StreamMessage';
  import { i18n, t } from '$lib/i18n';
  import type { StreamRun } from '$lib/stores/streams.svelte';
  import Button from '$lib/ui/Button.svelte';
  import CodeEditor from '$lib/ui/code/CodeEditor.svelte';
  import EmptyState from '$lib/ui/EmptyState.svelte';
  import Icon from '$lib/ui/Icon.svelte';
  import IconButton from '$lib/ui/IconButton.svelte';
  import Input from '$lib/ui/Input.svelte';
  import Select from '$lib/ui/overlay/Select.svelte';
  import Spinner from '$lib/ui/Spinner.svelte';
  import Tabs from '$lib/ui/Tabs.svelte';
  import VirtualList from '$lib/ui/VirtualList.svelte';
  import {
    clockTime,
    eventTypes,
    filterMessages,
    prettyJson,
    preview,
    type DirectionFilter,
  } from './log';

  interface Props {
    run: StreamRun;
    onclear: () => void;
  }

  let { run, onclear }: Props = $props();

  type Section = 'messages' | 'headers';
  let section = $state<Section>('messages');
  let text = $state('');
  let direction = $state<DirectionFilter>('all');
  let event = $state('');
  let selected = $state<number | null>(null);
  let viewport = $state<HTMLDivElement | null>(null);
  let follow = $state(true);

  const ROW = 26;
  const types = $derived(eventTypes(run.messages));
  const shown = $derived(filterMessages(run.messages, { text, direction, event }));
  const current = $derived(shown.find((m) => m.seq === selected) ?? null);
  const pretty = $derived(current ? prettyJson(current.data) : null);
  const status = $derived(run.info?.status ?? 'closed');

  // Keep the newest message in view unless the user scrolled up.
  $effect(() => {
    void shown.length;
    if (follow && viewport) {
      queueMicrotask(() => {
        if (viewport) viewport.scrollTop = viewport.scrollHeight;
      });
    }
  });

  // VirtualList owns `onscroll`; listen alongside it.
  $effect(() => {
    const v = viewport;
    if (!v) return;
    const onscroll = () => {
      follow = v.scrollTop + v.clientHeight >= v.scrollHeight - ROW;
    };
    v.addEventListener('scroll', onscroll);
    return () => v.removeEventListener('scroll', onscroll);
  });

  const statusLabel: Record<string, () => string> = {
    connecting: () => t('stream.connecting'),
    open: () => t('stream.open'),
    reconnecting: () => t('stream.reconnecting'),
    closed: () => t('stream.closed'),
  };

  function icon(m: StreamMessage) {
    return m.direction === 'sent' ? 'arrow-up' : m.direction === 'received' ? 'arrow-down' : 'info';
  }

  function badge(m: StreamMessage) {
    if (m.event) return m.event;
    if (m.kind === 'text' || m.kind === 'data') return '';
    return m.kind;
  }
</script>

<div class="stream">
  <header class="status">
    {#if run.connecting}
      <Spinner size={12} />
    {:else}
      <span class="dot" data-status={status}></span>
    {/if}
    <span class="state">{statusLabel[status]?.() ?? status}</span>
    {#if run.info}
      <span class="url" title={run.info.url}>{run.info.url}</span>
      {#if run.info.protocol}<span class="muted">{run.info.protocol}</span>{/if}
    {/if}
    <span class="spacer"></span>
    <span class="muted">{t('stream.count', { count: run.messages.length })}</span>
    <IconButton icon="trash" label={t('stream.clear')} onclick={onclear} />
  </header>

  <div class="tabs">
    <Tabs
      label={t('stream.sections')}
      idPrefix="stream"
      tabs={[
        { id: 'messages', label: t('stream.messages'), count: run.messages.length },
        { id: 'headers', label: t('stream.headers'), count: run.info?.headers.length ?? 0 },
      ]}
      bind:value={section}
    />
  </div>

  {#if section === 'headers'}
    <div class="headers">
      {#if run.info?.headers.length}
        <table>
          <tbody>
            {#each run.info.headers as [k, v], i (i)}
              <tr><th>{k}</th><td>{v}</td></tr>
            {/each}
          </tbody>
        </table>
      {:else}
        <EmptyState title={t('stream.noHeaders')} />
      {/if}
    </div>
  {:else if run.messages.length === 0}
    <EmptyState title={run.open ? t('stream.waiting') : t('stream.empty')} />
  {:else}
    <div class="filters">
      <Input
        value={text}
        placeholder={t('stream.filter')}
        aria-label={t('stream.filter')}
        oninput={(e) => (text = e.currentTarget.value)}
      />
      <Select
        label={t('stream.direction')}
        compact
        value={direction}
        options={[
          { value: 'all', label: t('stream.all') },
          { value: 'sent', label: t('stream.sent') },
          { value: 'received', label: t('stream.received') },
        ]}
        onchange={(v) => (direction = v)}
      />
      {#if types.length > 1}
        <Select
          label={t('stream.eventType')}
          compact
          value={event}
          options={[
            { value: '', label: t('stream.allEvents') },
            ...types.map((ty) => ({ value: ty, label: ty })),
          ]}
          onchange={(v) => (event = v)}
        />
      {/if}
    </div>
    <div class="body" class:split={current}>
      <VirtualList
        items={shown}
        rowHeight={ROW}
        key={(m) => m.seq}
        bind:viewport
        role="listbox"
        aria-label={t('stream.messages')}
      >
        {#snippet row(m: StreamMessage)}
          <button
            type="button"
            class="row"
            class:active={m.seq === selected}
            data-direction={m.direction}
            role="option"
            aria-selected={m.seq === selected}
            onclick={() => (selected = selected === m.seq ? null : m.seq)}
          >
            <span class="icon"><Icon name={icon(m)} size={12} /></span>
            <span class="time">{clockTime(m.time)}</span>
            {#if badge(m)}<span class="badge">{badge(m)}</span>{/if}
            <span class="data">{preview(m)}</span>
            <span class="size">{i18n.formatBytes(m.size)}</span>
          </button>
        {/snippet}
      </VirtualList>
      {#if current}
        <div class="detail">
          <div class="detail-bar">
            <span class="muted"
              >{clockTime(current.time)} · {i18n.formatBytes(current.size)}{current.eventId
                ? ` · id ${current.eventId}`
                : ''}</span
            >
            <span class="spacer"></span>
            <Button
              size="sm"
              variant="ghost"
              onclick={() => navigator.clipboard?.writeText(current.data)}
              >{t('stream.copy')}</Button
            >
          </div>
          <div class="viewer">
            <CodeEditor
              label={t('stream.message')}
              value={pretty ?? current.data}
              language={pretty ? 'json' : 'text'}
              readonly
              wrap
            />
          </div>
        </div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .stream {
    height: 100%;
    display: flex;
    flex-direction: column;
    min-height: 0;
  }
  .status {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-3);
    border-bottom: 1px solid var(--c-border);
    min-height: 40px;
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--c-fg-subtle);
    flex: none;
  }
  .dot[data-status='open'] {
    background: var(--c-success, #22c55e);
  }
  .dot[data-status='connecting'],
  .dot[data-status='reconnecting'] {
    background: var(--c-warning, #f59e0b);
  }
  .state {
    font-weight: 600;
  }
  .url {
    font-family: var(--font-mono);
    font-size: var(--font-size-xs);
    color: var(--c-fg-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }
  .muted {
    color: var(--c-fg-subtle);
    font-size: var(--font-size-xs);
  }
  .spacer {
    flex: 1;
  }
  .tabs {
    padding: 0 var(--space-2);
    border-bottom: 1px solid var(--c-border);
  }
  .filters {
    display: flex;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-3);
    border-bottom: 1px solid var(--c-border);
  }
  .filters :global(.input) {
    flex: 1;
  }
  .body {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-rows: 1fr;
  }
  .body.split {
    grid-template-rows: 1fr 1fr;
  }
  .row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    width: 100%;
    height: 26px;
    padding: 0 var(--space-3);
    text-align: left;
    font-size: var(--font-size-sm);
    border-bottom: 1px solid var(--c-border-subtle, transparent);
  }
  .row:hover {
    background: var(--c-bg-hover);
  }
  .row.active {
    background: var(--c-bg-active, var(--c-bg-hover));
  }
  .row[data-direction='sent'] .icon {
    color: var(--c-accent);
  }
  .row[data-direction='received'] .icon {
    color: var(--c-success, #22c55e);
  }
  .row[data-direction='info'] {
    color: var(--c-fg-muted);
    font-style: italic;
  }
  .icon {
    display: inline-flex;
    flex: none;
  }
  .time,
  .size {
    flex: none;
    font-family: var(--font-mono);
    font-size: var(--font-size-xs);
    color: var(--c-fg-subtle);
  }
  .badge {
    flex: none;
    padding: 0 var(--space-1);
    border-radius: var(--radius-sm);
    background: var(--c-bg-sunken);
    font-size: var(--font-size-xs);
    color: var(--c-fg-muted);
  }
  .data {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: var(--font-mono);
    font-size: var(--font-size-xs);
  }
  .detail {
    display: flex;
    flex-direction: column;
    min-height: 0;
    border-top: 1px solid var(--c-border);
  }
  .detail-bar {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-1) var(--space-3);
  }
  .viewer {
    flex: 1;
    min-height: 0;
  }
  .headers {
    flex: 1;
    overflow: auto;
    padding: var(--space-3);
  }
  table {
    border-collapse: collapse;
    font-size: var(--font-size-sm);
  }
  th {
    text-align: left;
    padding: var(--space-1) var(--space-3) var(--space-1) 0;
    color: var(--c-fg-muted);
    font-weight: 500;
    vertical-align: top;
  }
  td {
    font-family: var(--font-mono);
    font-size: var(--font-size-xs);
    word-break: break-all;
  }
</style>
