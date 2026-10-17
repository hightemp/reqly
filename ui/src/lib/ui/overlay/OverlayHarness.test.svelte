<script lang="ts">
  import Combobox from './Combobox.svelte';
  import OverlayHost from './OverlayHost.svelte';
  import Select from './Select.svelte';
  import { tooltip } from './tooltip.svelte';

  let { onchange }: { onchange?: (v: string) => void } = $props();
  let theme = $state('light');
  let method = $state('GET');
</script>

<OverlayHost />
<span data-testid="tip-target" use:tooltip={'Helpful hint'}>?</span>
<Select
  label="Theme"
  bind:value={theme}
  options={[
    { value: 'light', label: 'Light' },
    { value: 'dark', label: 'Dark' },
  ]}
  {onchange}
/>
<Combobox
  label="Method"
  bind:value={method}
  options={['GET', 'POST', 'PUT', 'PATCH', 'DELETE']}
  allowCustom
/>
<span data-testid="combo-value">{method}</span>
