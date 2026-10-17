<!-- Response of the active request: status line, history, Body/Headers/Cookies/Timeline. -->
<script lang="ts">
  import { onDestroy } from 'svelte';
  import type { TimelineEntry } from '$lib/api/generated/TimelineEntry';
  import type { Timings } from '$lib/api/generated/Timings';
  import { errorMessage } from '$lib/errors';
  import { i18n, t } from '$lib/i18n';
  import type { Services } from '$lib/services.svelte';
  import Button from '$lib/ui/Button.svelte';
  import EmptyState from '$lib/ui/EmptyState.svelte';
  import IconButton from '$lib/ui/IconButton.svelte';
  import { dialogs } from '$lib/ui/overlay/dialogs.svelte';
  import { menus } from '$lib/ui/overlay/menus.svelte';
  import Select from '$lib/ui/overlay/Select.svelte';
  import { tooltip } from '$lib/ui/overlay/tooltip.svelte';
  import ProgressBar from '$lib/ui/ProgressBar.svelte';
  import Spinner from '$lib/ui/Spinner.svelte';
  import StatusBadge from '$lib/ui/StatusBadge.svelte';
  import Tabs from '$lib/ui/Tabs.svelte';
  import { parseSetCookie } from './body';
  import { streamKind } from '$lib/protocols';
  import ScriptConsole from '../scripts/ScriptConsole.svelte';
  import TestResults from '../scripts/TestResults.svelte';
  import StreamLog from '../stream/StreamLog.svelte';
  import BodyView from './BodyView.svelte';

  let { services }: { services: Services } = $props();

  type Section = 'body' | 'headers' | 'cookies' | 'timeline' | 'console' | 'tests';
  let section = $state<Section>('body');
  let now = $state(Date.now());
  const timer = setInterval(() => (now = Date.now()), 100);
  onDestroy(() => clearInterval(timer));

  const requestId = $derived(services.tabs.activeId);
  const streaming = $derived(
    requestId ? streamKind(services.editor.get(requestId)?.node?.data) !== null : false,
  );
  const streamRun = $derived(requestId && streaming ? services.streams.run(requestId) : null);
  $effect(() => {
    if (requestId && streaming) void services.streams.load(requestId);
  });
  const run = $derived(requestId ? services.responses.run(requestId) : null);
  const record = $derived(run?.current ?? null);
  const timings = $derived((record?.timings ?? null) as Timings | null);
  const timeline = $derived((record?.timeline ?? []) as TimelineEntry[]);
  const setCookies = $derived(
    (record?.headers ?? [])
      .filter(([k]) => k.toLowerCase() === 'set-cookie')
      .map(([, v]) => parseSetCookie(v)),
  );

  $effect(() => {
    if (requestId) void services.responses.load(requestId);
  });

  // Console and Tests exist only for responses that have them.
  $effect(() => {
    if (
      (section === 'console' && !record?.scriptLogs.length) ||
      (section === 'tests' && !record?.tests.length)
    )
      section = 'body';
  });

  const phases = $derived.by(() => {
    if (!timings) return '';
    const f = (ms: number | null | undefined) => (ms == null ? '—' : i18n.formatDuration(ms));
    return [
      `${t('response.phaseDns')}: ${f(timings.dnsMs)}`,
      `${t('response.phaseConnect')}: ${f(timings.connectMs)}`,
      `${t('response.phaseTls')}: ${f(timings.tlsMs)}`,
      `${t('response.phaseTtfb')}: ${f(timings.ttfbMs)}`,
      `${t('response.phaseTotal')}: ${f(timings.totalMs)}`,
    ].join(' · ');
  });

  function historyLabel(h: {
    createdAt: number;
    status: number | null;
    totalMs: number | null;
    errorCode: string | null;
  }) {
    const when = i18n.formatDateTime(h.createdAt, { timeStyle: 'medium', dateStyle: 'short' });
    const status = h.status ?? t('response.failed');
    const ms = h.totalMs != null ? ` · ${i18n.formatDuration(h.totalMs)}` : '';
    return `${when} · ${status}${ms}`;
  }

  function actions(anchor: HTMLElement) {
    if (!requestId || !record) return;
    const id = requestId;
    const rec = record;
    menus.openBelow(anchor, [
      {
        id: 'delete',
        label: t('response.deleteOne'),
        onSelect: () => void services.responses.deleteResponse(id, rec.id),
      },
      {
        id: 'clear',
        label: t('response.clearHistory'),
        danger: true,
        onSelect: async () => {
          const ok = await dialogs.confirm({
            title: t('response.clearHistory'),
            message: t('response.clearMessage'),
            danger: true,
            confirmLabel: t('tree.delete'),
          });
          if (ok) await services.responses.clear(id);
        },
      },
    ]);
  }

  const kindClass: Record<string, string> = {
    info: 'info',
    headerOut: 'out',
    dataOut: 'out',
    headerIn: 'in',
  };
  const prefix: Record<string, string> = { info: '*', headerOut: '>', dataOut: '|', headerIn: '<' };
</script>

{#if requestId && streamRun}
  <StreamLog run={streamRun} onclear={() => void services.streams.clear(requestId)} />
{:else if !requestId || !run}
  <EmptyState title={t('response.empty')} />
{:else}
  <div class="response">
    <header class="status">
      {#if run.sending}
        <Spinner size={12} />
        <span class="muted">{t('response.sending')}</span>
        <span class="metric">{i18n.formatDuration(now - (run.startedAt ?? now))}</span>
        {#if run.progress}<span class="metric">{i18n.formatBytes(run.progress.downloaded)}</span
          >{/if}
        <span class="spacer"></span>
        <Button size="sm" onclick={() => void services.responses.cancel(requestId)}
          >{t('request.cancel')}</Button
        >
      {:else if record}
        {#if record.error}
          <StatusBadge status={null} text={t('response.error')} />
        {:else}
          <StatusBadge status={record.status} text={record.statusText} />
        {/if}
        {#if timings}<span class="metric" use:tooltip={phases}
            >{i18n.formatDuration(timings.totalMs)}</span
          >{/if}
        <span
          class="metric"
          use:tooltip={t('response.sizeHint', { headers: i18n.formatBytes(record.headersSize) })}
        >
          {i18n.formatBytes(record.bodySize)}
        </span>
        <span class="spacer"></span>
        <div class="history">
          <Select
            compact
            label={t('response.history')}
            value={record.id}
            options={run.history.map((h) => ({ value: h.id, label: historyLabel(h) }))}
            onchange={(id) => void services.responses.select(requestId, id)}
          />
        </div>
        <IconButton
          icon="more"
          size={14}
          label={t('response.actions')}
          onclick={(e) => actions(e.currentTarget)}
        />
      {/if}
    </header>
    {#if run.sending}<ProgressBar
        value={run.progress?.total ? run.progress.downloaded / run.progress.total : undefined}
      />{/if}

    {#if !record}
      <EmptyState title={run.sending ? t('response.waiting') : t('response.empty')} />
    {:else}
      <div class="tabs">
        <Tabs
          label={t('response.sections')}
          idPrefix="response"
          bind:value={section}
          tabs={[
            { id: 'body', label: t('response.body') },
            { id: 'headers', label: t('response.headers'), count: record.headers.length },
            { id: 'cookies', label: t('response.cookies'), count: setCookies.length },
            { id: 'timeline', label: t('response.timeline') },
            ...(record.tests.length
              ? [
                  {
                    id: 'tests' as const,
                    label: t('scripts.tests'),
                    count: record.tests.length,
                    dot: record.tests.some((x) => !x.passed),
                  },
                ]
              : []),
            ...(record.scriptLogs.length
              ? [
                  {
                    id: 'console' as const,
                    label: t('scripts.console'),
                    count: record.scriptLogs.length,
                  },
                ]
              : []),
          ]}
        />
      </div>
      <div
        class="panel"
        role="tabpanel"
        id="response-panel-{section}"
        aria-labelledby="response-tab-{section}"
      >
        {#if section === 'body'}
          {#if record.error}
            <div class="error selectable" role="alert">
              <h3>{errorMessage(record.error)}</h3>
              {#if record.error.params.detail}<pre>{String(record.error.params.detail)}</pre>{/if}
              <Button size="sm" onclick={() => (section = 'timeline')}
                >{t('response.showTimeline')}</Button
              >
            </div>
          {:else}
            <BodyView {services} {record} />
          {/if}
        {:else if section === 'headers'}
          <table class="table selectable">
            <tbody>
              {#each record.headers as [name, value], i (i)}
                <tr><th>{name}</th><td>{value}</td></tr>
              {/each}
            </tbody>
          </table>
        {:else if section === 'cookies'}
          {#if setCookies.length === 0}
            <EmptyState title={t('response.noCookies')} />
          {:else}
            <table class="table selectable">
              <thead>
                <tr>
                  <th>{t('cookies.name')}</th>
                  <th>{t('cookies.value')}</th>
                  <th>{t('cookies.domain')}</th>
                  <th>{t('cookies.path')}</th>
                  <th>{t('cookies.expires')}</th>
                  <th>{t('cookies.flags')}</th>
                </tr>
              </thead>
              <tbody>
                {#each setCookies as c, i (i)}
                  <tr>
                    <th>{c.name}</th>
                    <td>{c.value}</td>
                    <td>{c.domain ?? ''}</td>
                    <td>{c.path ?? ''}</td>
                    <td>{c.expires ?? (c.maxAge ? `max-age ${c.maxAge}` : t('cookies.session'))}</td
                    >
                    <td
                      >{[
                        c.secure && 'Secure',
                        c.httpOnly && 'HttpOnly',
                        c.sameSite && `SameSite=${c.sameSite}`,
                      ]
                        .filter(Boolean)
                        .join(', ')}</td
                    >
                  </tr>
                {/each}
              </tbody>
            </table>
          {/if}
        {:else if section === 'tests'}
          <TestResults tests={record.tests} />
        {:else if section === 'console'}
          <ScriptConsole logs={record.scriptLogs} />
        {:else}
          <div class="timeline selectable">
            {#each timeline as e, i (i)}
              <div class="line {kindClass[e.kind]}">
                <span class="mark">{prefix[e.kind]}</span>{e.text}
              </div>
            {/each}
          </div>
        {/if}
      </div>
    {/if}
  </div>
{/if}

<style>
  .response {
    height: 100%;
    display: flex;
    flex-direction: column;
    min-height: 0;
    background: var(--c-bg-elevated);
  }
  .status {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-height: 45px;
    padding: var(--space-2) var(--space-3);
    border-bottom: 1px solid var(--c-border);
  }
  .muted {
    color: var(--c-fg-muted);
  }
  .metric {
    padding: 2px 6px;
    border-radius: var(--radius-sm);
    background: var(--c-bg-active);
    font-size: var(--font-size-sm);
    color: var(--c-fg-muted);
    white-space: nowrap;
  }
  .spacer {
    flex: 1;
  }
  .history {
    max-width: 260px;
    min-width: 0;
  }
  .history :global(.select) {
    max-width: 100%;
    font-size: var(--font-size-xs);
  }
  .tabs {
    padding: 0 var(--space-2);
    border-bottom: 1px solid var(--c-border);
  }
  .panel {
    flex: 1;
    min-height: 0;
    overflow: auto;
  }
  .error {
    padding: var(--space-4);
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: var(--space-3);
  }
  .error h3 {
    color: var(--c-danger);
    font-weight: 600;
  }
  .error pre {
    white-space: pre-wrap;
    font-family: var(--font-mono);
    font-size: var(--font-size-sm);
    color: var(--c-fg-muted);
  }
  .table {
    width: 100%;
    border-collapse: collapse;
    font-family: var(--font-mono);
    font-size: var(--font-size-sm);
  }
  .table th,
  .table td {
    text-align: left;
    vertical-align: top;
    padding: 4px var(--space-3);
    border-bottom: 1px solid var(--c-border);
    word-break: break-all;
  }
  .table th {
    font-weight: 600;
    color: var(--c-syntax-property);
    white-space: nowrap;
  }
  .table thead th {
    color: var(--c-fg-muted);
    font-family: var(--font-ui);
  }
  .timeline {
    padding: var(--space-2) var(--space-3);
    font-family: var(--font-mono);
    font-size: var(--font-size-sm);
    line-height: 1.6;
  }
  .line {
    white-space: pre-wrap;
    word-break: break-all;
  }
  .mark {
    display: inline-block;
    width: 1.5em;
    color: var(--c-fg-subtle);
  }
  .info {
    color: var(--c-fg-subtle);
  }
  .out {
    color: var(--c-syntax-string);
  }
  .in {
    color: var(--c-syntax-keyword);
  }
</style>
