<script lang="ts">
  let { size = 14, label }: { size?: number; label?: string } = $props();
</script>

<span
  class="spinner"
  role="progressbar"
  aria-label={label}
  style:width="{size}px"
  style:height="{size}px"
></span>

<style>
  .spinner {
    display: inline-block;
    flex: none;
    border-radius: 50%;
    border: 2px solid currentColor;
    border-right-color: transparent;
    opacity: 0.8;
    animation: spin 0.7s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .spinner {
      animation-duration: 2s;
    }
  }
</style>
