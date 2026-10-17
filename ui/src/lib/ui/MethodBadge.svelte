<script lang="ts">
  import { methodColorVar, methodLabel } from './badges';

  let { method, full = false }: { method: string; full?: boolean } = $props();
</script>

<span class="method" style:color={methodColorVar(method)} title={method.toUpperCase()}>
  {full ? method.toUpperCase() : methodLabel(method)}
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
