<!-- Overlapping initials of people, colored per person. -->
<script lang="ts">
  import { userColor } from '$lib/collab/collab';

  interface Props {
    people: { userId: string; name: string }[];
    /** Circle size in px. */
    size?: number;
    /** At most this many circles; the rest becomes "+n". */
    max?: number;
    label: string;
  }

  let { people, size = 20, max = 4, label }: Props = $props();
  const shown = $derived(people.slice(0, max));
  const initials = (name: string) =>
    name
      .split(/\s+/)
      .filter(Boolean)
      .slice(0, 2)
      .map((w) => w[0]!.toUpperCase())
      .join('') || '?';
</script>

{#if people.length}
  <span
    class="avatars"
    role="img"
    aria-label={`${label}: ${people.map((p) => p.name).join(', ')}`}
    title={people.map((p) => p.name).join(', ')}
    style:--size={`${size}px`}
  >
    {#each shown as p (p.userId)}
      <span class="avatar" style:background={userColor(p.userId)}>{initials(p.name)}</span>
    {/each}
    {#if people.length > max}<span class="avatar more">+{people.length - max}</span>{/if}
  </span>
{/if}

<style>
  .avatars {
    display: inline-flex;
    flex: none;
    align-items: center;
  }
  .avatar {
    width: var(--size);
    height: var(--size);
    border-radius: 50%;
    display: grid;
    place-items: center;
    color: #fff;
    font-size: calc(var(--size) * 0.45);
    font-weight: 600;
    line-height: 1;
    border: 1.5px solid var(--c-bg-elevated);
  }
  .avatar + .avatar {
    margin-left: calc(var(--size) * -0.3);
  }
  .more {
    background: var(--c-fg-subtle);
  }
</style>
