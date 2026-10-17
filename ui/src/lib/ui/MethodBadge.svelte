<script lang="ts">
  import type { Protocol } from '$lib/api/generated/Protocol';
  import { requestBadge } from './badges';

  let {
    method,
    protocol = null,
    full = false,
  }: { method: string; protocol?: Protocol | null; full?: boolean } = $props();
  const badge = $derived(requestBadge(method, protocol));
</script>

<span class="method" style:color={badge.color} title={badge.title}>
  {full && (!protocol || protocol === 'http') ? method.toUpperCase() : badge.label}
</span>

<style>
  .method {
    flex: none;
    font-family: var(--font-mono);
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.02em;
    min-width: 34px;
    text-align: right;
  }
</style>
