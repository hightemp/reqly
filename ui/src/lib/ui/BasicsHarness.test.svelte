<script lang="ts">
  import Button from './Button.svelte';
  import Checkbox from './Checkbox.svelte';
  import Input from './Input.svelte';
  import MethodBadge from './MethodBadge.svelte';
  import ProgressBar from './ProgressBar.svelte';
  import StatusBadge from './StatusBadge.svelte';
  import Switch from './Switch.svelte';

  // i18n-ignore-file: test harness
  let { kind, args }: { kind: string; args: Record<string, unknown> } = $props();
  // svelte-ignore state_referenced_locally
  let value = $state((args.value as string) ?? '');
  // svelte-ignore state_referenced_locally
  let checked = $state((args.checked as boolean) ?? false);
</script>

{#if kind === 'button'}
  <Button {...args}>{args.label}</Button>
{:else if kind === 'input'}
  <Input bind:value aria-label={args.label as string} {...args} />
  <span data-testid="value">{value}</span>
{:else if kind === 'checkbox'}
  <Checkbox bind:checked label={args.label as string} disabled={args.disabled as boolean} />
{:else if kind === 'switch'}
  <Switch bind:checked label={args.label as string} />
{:else if kind === 'badges'}
  <MethodBadge method="DELETE" />
  <StatusBadge status={201} text="Created" />
{:else if kind === 'progress'}
  <ProgressBar value={args.value as number} />
  <ProgressBar />
{/if}
