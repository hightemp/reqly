<script lang="ts">
  import { statusClass } from './badges';

  let { status, text = '' }: { status: number | null; text?: string } = $props();
  const cls = $derived(statusClass(status));
</script>

<span class="status" style:--status-color="var(--c-status-{cls})">
  {status ?? ''}{text ? ` ${text}` : ''}
</span>

<style>
  .status {
    display: inline-flex;
    align-items: center;
    height: 22px;
    padding: 0 var(--space-2);
    border-radius: var(--radius-sm);
    font-weight: 600;
    font-size: var(--font-size-sm);
    color: var(--c-fg-on-accent);
    background: var(--status-color);
    white-space: nowrap;
  }
</style>
