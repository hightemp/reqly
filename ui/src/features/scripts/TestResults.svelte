<!-- Test results of a response's scripts. -->
<script lang="ts">
  import type { TestResult } from '$lib/api/generated/TestResult';
  import { t } from '$lib/i18n';
  import EmptyState from '$lib/ui/EmptyState.svelte';

  let { tests }: { tests: TestResult[] } = $props();
  const passed = $derived(tests.filter((x) => x.passed && !x.skipped).length);
  const failed = $derived(tests.filter((x) => !x.passed).length);
  const skipped = $derived(tests.filter((x) => x.skipped).length);
</script>

{#if tests.length === 0}
  <EmptyState title={t('scripts.noTests')} />
{:else}
  <div class="tests">
    <p class="summary">
      <span class="pass">{t('scripts.passed', { count: passed })}</span>
      {#if failed}<span class="fail">{t('scripts.failed', { count: failed })}</span>{/if}
      {#if skipped}<span class="skip">{t('scripts.skipped', { count: skipped })}</span>{/if}
    </p>
    <ul>
      {#each tests as test, i (i)}
        <li data-state={test.skipped ? 'skipped' : test.passed ? 'passed' : 'failed'}>
          <span class="mark">{test.skipped ? '–' : test.passed ? '✓' : '✗'}</span>
          <span class="name">{test.name}</span>
          {#if test.error}<span class="error selectable">{test.error}</span>{/if}
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
  .error {
    grid-column: 2;
    font-family: var(--font-mono);
    font-size: var(--font-size-xs);
  }
</style>
