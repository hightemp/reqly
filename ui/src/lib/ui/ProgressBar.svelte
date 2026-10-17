<script lang="ts">
  /** `value` in 0..1; omit for an indeterminate bar. */
  let { value, label }: { value?: number; label?: string } = $props();
  const percent = $derived(
    value === undefined ? undefined : Math.round(Math.min(1, Math.max(0, value)) * 100),
  );
</script>

<div
  class="progress"
  class:indeterminate={percent === undefined}
  role="progressbar"
  aria-label={label}
  aria-valuemin={percent === undefined ? undefined : 0}
  aria-valuemax={percent === undefined ? undefined : 100}
  aria-valuenow={percent}
>
  <div class="fill" style:width={percent === undefined ? undefined : `${percent}%`}></div>
</div>

<style>
  .progress {
    height: 2px;
    width: 100%;
    overflow: hidden;
    background: var(--c-accent-subtle);
  }
  .fill {
    height: 100%;
    background: var(--c-accent);
    transition: width var(--transition);
  }
  .indeterminate .fill {
    width: 30%;
    animation: slide 1.1s ease-in-out infinite;
  }
  @keyframes slide {
    from {
      transform: translateX(-100%);
    }
    to {
      transform: translateX(340%);
    }
  }
</style>
