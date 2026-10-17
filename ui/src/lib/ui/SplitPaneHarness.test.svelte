<script lang="ts">
  import SplitPane from './SplitPane.svelte';

  let { size = $bindable(200), ...rest }: Record<string, unknown> & { size?: number } = $props();
</script>

<SplitPane bind:size {...rest} label="test">
  {#snippet first()}<div data-testid="first"></div>{/snippet}
  {#snippet second()}<div data-testid="second"></div>{/snippet}
</SplitPane>
