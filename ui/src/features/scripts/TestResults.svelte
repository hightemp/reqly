<!-- Results of a response's assertions and script tests. -->
<script lang="ts">
  import type { AssertionResult } from '$lib/api/generated/AssertionResult';
  import type { TestResult } from '$lib/api/generated/TestResult';
  import { errorMessage } from '$lib/errors';
  import { t } from '$lib/i18n';
  import EmptyState from '$lib/ui/EmptyState.svelte';
  import { describeAssertion } from '../tests/assertions';

  let { tests, assertions = [] }: { tests: TestResult[]; assertions?: AssertionResult[] } =
    $props();

  interface Row {
    name: string;
    state: 'passed' | 'failed' | 'skipped';
    /** What came back, for failed assertions. */
    got?: string;
    error?: string;
    detail?: string;
  }

  const rows = $derived<Row[]>([
    ...assertions.map((a) => ({
      name: describeAssertion(a),
      state: a.passed ? ('passed' as const) : ('failed' as const),
      got:
        a.passed || a.error
          ? undefined
          : a.actual != null
            ? t('assertions.got', { value: a.actual })
            : t('assertions.noValue'),
      error: a.error ? errorMessage(a.error) : undefined,
      detail: a.detail,
    })),
    ...tests.map((x) => ({
      name: x.name,
      state: x.skipped
        ? ('skipped' as const)
        : x.passed
          ? ('passed' as const)
          : ('failed' as const),
      error: x.error,
    })),
  ]);
  const passed = $derived(rows.filter((r) => r.state === 'passed').length);
  const failed = $derived(rows.filter((r) => r.state === 'failed').length);
  const skipped = $derived(rows.filter((r) => r.state === 'skipped').length);
</script>

{#if rows.length === 0}
  <EmptyState title={t('scripts.noTests')} />
{:else}
  <div class="tests">
    <p class="summary">
      <span class="pass">{t('scripts.passed', { count: passed })}</span>
      {#if failed}<span class="fail">{t('scripts.failed', { count: failed })}</span>{/if}
      {#if skipped}<span class="skip">{t('scripts.skipped', { count: skipped })}</span>{/if}
    </p>
    <ul>
      {#each rows as row, i (i)}
        <li data-state={row.state}>
          <span class="mark"
            >{row.state === 'skipped' ? '–' : row.state === 'passed' ? '✓' : '✗'}</span
          >
          <span class="name">{row.name}</span>
          {#if row.got}<span class="got selectable">{row.got}</span>{/if}
          {#if row.error}<span class="error selectable">{row.error}</span>{/if}
          {#if row.detail}<pre class="error selectable">{row.detail}</pre>{/if}
        </li>
      {/each}
    </ul>
  </div>
{/if}

<style>
  .summary {
    display: flex;
    gap: var(--space-3);
    margin-bottom: var(--space-2);
    font-weight: 600;
  }
  .pass {
    color: var(--c-success, #16a34a);
  }
  .fail {
    color: var(--c-danger);
  }
  .skip {
    color: var(--c-fg-subtle);
  }
  ul {
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }
  li {
    display: grid;
    grid-template-columns: 16px 1fr;
    column-gap: var(--space-2);
  }
  li[data-state='passed'] .mark {
    color: var(--c-success, #16a34a);
  }
  li[data-state='failed'] .mark,
  .error {
    color: var(--c-danger);
  }
  li[data-state='skipped'] {
    color: var(--c-fg-subtle);
  }
  .got,
  .error {
    grid-column: 2;
    font-family: var(--font-mono);
    font-size: var(--font-size-xs);
  }
  .got {
    color: var(--c-fg-muted);
  }
  pre.error {
    margin: 0;
    white-space: pre-wrap;
  }
</style>
