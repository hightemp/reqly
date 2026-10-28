<!-- Collection runner: options, then live results of the run. -->
<script lang="ts">
  import type { Environment } from '$lib/api/generated/Environment';
  import type { ExportFile } from '$lib/api/generated/ExportFile';
  import type { ReportFormat } from '$lib/api/generated/ReportFormat';
  import type { RunItem } from '$lib/api/generated/RunItem';
  import { errorMessage } from '$lib/errors';
  import { i18n, t } from '$lib/i18n';
  import { browserChoose } from '$lib/platform/files';
  import type { RunnerStore } from '$lib/stores/runner.svelte';
  import Button from '$lib/ui/Button.svelte';
  import Checkbox from '$lib/ui/Checkbox.svelte';
  import IconButton from '$lib/ui/IconButton.svelte';
  import Input from '$lib/ui/Input.svelte';
  import MethodBadge from '$lib/ui/MethodBadge.svelte';
  import Modal from '$lib/ui/overlay/Modal.svelte';
  import Select from '$lib/ui/overlay/Select.svelte';
  import { toasts } from '$lib/ui/overlay/toasts.svelte';
  import ProgressBar from '$lib/ui/ProgressBar.svelte';
  import TestResults from '../scripts/TestResults.svelte';
  import type { PlannedRequest } from './plan';

  interface Props {
    runner: RunnerStore;
    requests: PlannedRequest[];
    environments: Environment[];
    activeEnvironment: Environment | null;
    /** Saves pending edits before a run. */
    onbeforerun?: () => Promise<void>;
    /** Saves a report file; false when the user cancelled. */
    onsave?: (file: ExportFile) => Promise<boolean>;
  }

  let { runner, requests, environments, activeEnvironment, onbeforerun, onsave }: Props = $props();

  const REPORT_MIME: Record<ReportFormat, string> = {
    json: 'application/json',
    junit: 'application/xml',
    html: 'text/html',
  };
  const reportFormats = $derived<{ value: ReportFormat; label: string }[]>([
    { value: 'html', label: t('runner.formats.html') },
    { value: 'junit', label: t('runner.formats.junit') },
    { value: 'json', label: t('runner.formats.json') },
  ]);
  let reportFormat = $state<ReportFormat>('html');

  // svelte-ignore state_referenced_locally
  let order = $state(requests.map((r) => ({ ...r, selected: true })));
  let environmentId = $state('active');
  let iterations = $state('');
  let delay = $state('0');
  let stopOnFailure = $state(false);
  let data = $state<{ name: string; format: 'csv' | 'json'; text: string; rows: number } | null>(
    null,
  );
  let expanded = $state<number | null>(null);

  const target = $derived(runner.target);
  const selected = $derived(order.filter((r) => r.selected));
  const envOptions = $derived([
    {
      value: 'active',
      label: activeEnvironment
        ? t('runner.activeEnvironment', { name: activeEnvironment.name })
        : t('runner.noEnvironment'),
    },
    ...environments
      .filter((e) => e.id !== activeEnvironment?.id)
      .map((e) => ({ value: e.id, label: e.name })),
  ]);
  const items = $derived(runner.items);
  const passed = $derived(items.filter(ok).length);
  const failed = $derived(items.length - passed);
  const checks = $derived(items.reduce((n, i) => n + i.tests.length + i.assertions.length, 0));
  const multiIteration = $derived(
    (runner.report?.iterations ?? 0) > 1 || items.some((i) => i.iteration > 0),
  );

  function ok(i: RunItem): boolean {
    return i.error == null && i.tests.every((x) => x.passed) && i.assertions.every((a) => a.passed);
  }

  function move(index: number, delta: number) {
    const next = [...order];
    const [row] = next.splice(index, 1);
    next.splice(index + delta, 0, row!);
    order = next;
  }

  function selectAll(on: boolean) {
    order = order.map((r) => ({ ...r, selected: on }));
  }

  async function chooseData() {
    const file = await browserChoose();
    if (!file) return;
    const ext = file.name.split('.').pop()?.toLowerCase();
    if (ext !== 'csv' && ext !== 'json') {
      toasts.error(t('runner.dataUnsupported'));
      return;
    }
    const text = await file.text();
    let rows: number;
    if (ext === 'json') {
      try {
        const parsed: unknown = JSON.parse(text);
        rows = Array.isArray(parsed) ? parsed.length : 0;
      } catch {
        rows = 0;
      }
    } else {
      rows = Math.max(0, text.split(/\r?\n/).filter((l) => l.trim()).length - 1);
    }
    data = { name: file.name, format: ext, text, rows };
  }

  async function run() {
    if (!target) return;
    await onbeforerun?.();
    const count = Number.parseInt(iterations, 10);
    const pause = Number.parseInt(delay, 10);
    expanded = null;
    await runner.start({
      collectionId: target.collectionId,
      requestIds: selected.map((r) => r.id),
      environmentId: environmentId === 'active' ? undefined : environmentId,
      iterations: count > 0 ? count : undefined,
      data: data ? { format: data.format, text: data.text } : undefined,
      delayMs: pause > 0 ? pause : undefined,
      stopOnFailure,
    });
  }

  async function saveReport() {
    const report = runner.report;
    if (!report || !onsave) return;
    try {
      const out = await runner.renderReport(reportFormat);
      if (!out) return;
      const saved = await onsave({
        name: out.fileName,
        mime: REPORT_MIME[reportFormat],
        content: out.text,
      });
      if (saved) toasts.show({ message: t('runner.reportSaved'), kind: 'success' });
    } catch (e) {
      toasts.error(errorMessage(e));
    }
  }

  function statusText(i: RunItem): string {
    if (i.error) return errorMessage(i.error);
    return i.status != null ? String(i.status) : '';
  }
</script>

<Modal
  title={t('runner.title', { name: target?.name ?? '' })}
  width="760px"
  onclose={() => runner.close()}
>
  {#if runner.status === 'idle'}
    <div class="options">
      <section class="requests">
        <div class="heading">
          <h3>{t('runner.requests')}</h3>
          <button type="button" class="link" onclick={() => selectAll(true)}
            >{t('runner.selectAll')}</button
          >
          <button type="button" class="link" onclick={() => selectAll(false)}
            >{t('runner.selectNone')}</button
          >
        </div>
        {#if order.length === 0}
          <p class="hint">{t('runner.noRequests')}</p>
        {:else}
          <ul aria-label={t('runner.requests')}>
            {#each order as r, i (r.id)}
              <li>
                <Checkbox
                  label={r.name}
                  checked={r.selected}
                  onchange={(on) =>
                    (order = order.map((x, j) => (j === i ? { ...x, selected: on } : x)))}
                />
                <MethodBadge method={r.method} />
                <span class="name">{r.name}</span>
                {#if r.path.length}<span class="path">{r.path.join(' / ')}</span>{/if}
                <span class="moves">
                  <IconButton
                    icon="arrow-up"
                    label={t('runner.moveUp')}
                    disabled={i === 0}
                    onclick={() => move(i, -1)}
                  />
                  <IconButton
                    icon="arrow-down"
                    label={t('runner.moveDown')}
                    disabled={i === order.length - 1}
                    onclick={() => move(i, 1)}
                  />
                </span>
              </li>
            {/each}
          </ul>
        {/if}
      </section>
      <section class="settings">
        <label class="field">
          <span>{t('runner.environment')}</span>
          <Select label={t('runner.environment')} bind:value={environmentId} options={envOptions} />
        </label>
        <label class="field">
          <span>{t('runner.iterations')}</span>
          <Input
            type="number"
            min="1"
            bind:value={iterations}
            aria-label={t('runner.iterations')}
            placeholder={data ? t('runner.iterationsAuto') : '1'}
          />
        </label>
        <div class="field">
          <span>{t('runner.data')}</span>
          {#if data}
            <span class="data">
              <span>{t('runner.dataRows', { name: data.name, count: data.rows })}</span>
              <IconButton
                icon="close"
                label={t('runner.removeData')}
                onclick={() => (data = null)}
              />
            </span>
          {:else}
            <Button size="sm" onclick={() => void chooseData()}>{t('runner.chooseData')}</Button>
          {/if}
        </div>
        <label class="field">
          <span>{t('runner.delay')}</span>
          <Input type="number" min="0" bind:value={delay} aria-label={t('runner.delay')} />
        </label>
        <Checkbox label={t('runner.stopOnFailure')} showLabel bind:checked={stopOnFailure} />
      </section>
    </div>
  {:else}
    <div class="results">
      <div class="summary">
        <ProgressBar
          label={t('runner.progress')}
          value={runner.status === 'running'
            ? runner.total
              ? items.length / runner.total
              : undefined
            : 1}
        />
        <p>
          <span class="pass">{t('runner.requestsPassed', { count: passed })}</span>
          {#if failed}<span class="fail">{t('runner.requestsFailed', { count: failed })}</span>{/if}
          <span>{t('runner.checks', { count: checks })}</span>
          {#if runner.report}
            <span
              >{t('runner.took', {
                time: i18n.formatDuration(runner.report.finishedAt - runner.report.startedAt),
              })}</span
            >
          {/if}
          {#if runner.report?.cancelled}<span class="fail">{t('runner.cancelled')}</span>{/if}
          {#if runner.report?.stopped}<span class="fail">{t('runner.stopped')}</span>{/if}
        </p>
        {#if runner.error}<p class="fail">{runner.error}</p>{/if}
      </div>
      {#if runner.status === 'done' && items.length === 0 && !runner.error}
        <p class="hint">{t('runner.noResults')}</p>
      {/if}
      <ul aria-label={t('runner.results')}>
        {#each items as it, i (i)}
          {#if multiIteration && (i === 0 || items[i - 1]!.iteration !== it.iteration)}
            <li class="iteration">{t('runner.iteration', { n: it.iteration + 1 })}</li>
          {/if}
          <li class="item" data-state={ok(it) ? 'passed' : 'failed'}>
            <button
              type="button"
              class="row"
              aria-expanded={expanded === i}
              onclick={() => (expanded = expanded === i ? null : i)}
            >
              <span class="mark">{ok(it) ? '✓' : '✗'}</span>
              <MethodBadge method={it.method} />
              <span class="name">{it.name}</span>
              <span class="status">{statusText(it)}</span>
              {#if it.totalMs != null}<span class="time">{i18n.formatDuration(it.totalMs)}</span
                >{/if}
              <span class="count"
                >{t('runner.checks', { count: it.tests.length + it.assertions.length })}</span
              >
            </button>
            {#if expanded === i}
              <div class="details">
                <p class="url selectable">{it.url}</p>
                <TestResults tests={it.tests} assertions={it.assertions} />
              </div>
            {/if}
          </li>
        {/each}
      </ul>
    </div>
  {/if}

  {#snippet footer()}
    {#if runner.status === 'idle'}
      <Button onclick={() => runner.close()}>{t('common.cancel')}</Button>
      <Button variant="primary" disabled={selected.length === 0} onclick={() => void run()}>
        {t('runner.run')}
      </Button>
    {:else if runner.status === 'running'}
      <Button variant="danger" onclick={() => void runner.cancel()}>{t('runner.stop')}</Button>
    {:else}
      {#if runner.report && onsave}
        <div class="report">
          <Select
            label={t('runner.reportFormat')}
            compact
            bind:value={reportFormat}
            options={reportFormats}
          />
          <Button onclick={() => void saveReport()}>{t('runner.saveReport')}</Button>
        </div>
      {/if}
      {#if runner.report && items.some((i) => !ok(i))}
        <Button onclick={() => void runner.rerunFailed()}>{t('runner.rerunFailed')}</Button>
      {/if}
      <Button onclick={() => runner.reset()}>{t('runner.newRun')}</Button>
      <Button variant="primary" onclick={() => runner.close()}>{t('common.close')}</Button>
    {/if}
  {/snippet}
</Modal>

<style>
  .options {
    display: grid;
    grid-template-columns: minmax(0, 3fr) minmax(0, 2fr);
    gap: var(--space-4);
  }
  @media (max-width: 767px) {
    .options {
      grid-template-columns: minmax(0, 1fr);
    }
  }
  .heading {
    display: flex;
    align-items: baseline;
    gap: var(--space-2);
    margin-bottom: var(--space-2);
  }
  h3 {
    font-size: var(--font-size-sm);
    font-weight: 600;
    margin-right: auto;
  }
  .link {
    color: var(--c-accent);
    font-size: var(--font-size-xs);
  }
  ul {
    list-style: none;
    display: flex;
    flex-direction: column;
    max-height: 50vh;
    overflow: auto;
  }
  .requests li {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: 2px 0;
  }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .path,
  .hint,
  .time,
  .count,
  .url {
    color: var(--c-fg-muted);
    font-size: var(--font-size-xs);
  }
  .path {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .moves {
    margin-left: auto;
    display: flex;
  }
  .settings {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    font-size: var(--font-size-sm);
  }
  .data {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .summary {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    margin-bottom: var(--space-3);
  }
  .summary p {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-3);
  }
  .pass {
    color: var(--c-success, #16a34a);
  }
  .fail {
    color: var(--c-danger);
  }
  .iteration {
    font-weight: 600;
    font-size: var(--font-size-xs);
    color: var(--c-fg-muted);
    padding: var(--space-2) 0 var(--space-1);
  }
  .row {
    width: 100%;
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-1) var(--space-2);
    border-radius: var(--radius-sm);
    text-align: left;
  }
  .row:hover {
    background: var(--c-bg-hover);
  }
  .item[data-state='passed'] .mark {
    color: var(--c-success, #16a34a);
  }
  .item[data-state='failed'] .mark,
  .item[data-state='failed'] .status {
    color: var(--c-danger);
  }
  .status {
    margin-left: auto;
    font-family: var(--font-mono);
    font-size: var(--font-size-xs);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 40%;
  }
  .details {
    padding: var(--space-2) var(--space-2) var(--space-3) calc(var(--space-2) + 16px);
  }
  .report {
    display: flex;
    gap: var(--space-2);
    margin-right: auto;
  }
  .url {
    font-family: var(--font-mono);
    margin-bottom: var(--space-2);
    word-break: break-all;
  }
</style>
