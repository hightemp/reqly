<script lang="ts">
  let { keys }: { keys: string } = $props();
</script>

<kbd class="kbd">{keys}</kbd>

<style>
  .kbd {
    display: inline-block;
    padding: 1px 6px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--c-border-strong);
    background: var(--c-bg-sunken);
    color: var(--c-fg-muted);
    font-family: var(--font-mono);
    font-size: var(--font-size-xs);
    white-space: nowrap;
  }
</style>
