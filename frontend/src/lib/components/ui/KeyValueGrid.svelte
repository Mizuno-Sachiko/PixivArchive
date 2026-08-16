<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    children: Snippet;
    variant?: 'summary' | 'metrics';
    class?: string;
  }

  let {
    children,
    variant = 'summary',
    class: className = ''
  }: Props = $props();
</script>

<dl class={`key-value-grid ${variant} ${className}`.trim()}>
  {@render children()}
</dl>

<style>
  .key-value-grid {
    display: grid;
    gap: 0.55rem;
    margin: 0;
  }

  .key-value-grid.summary {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }

  .key-value-grid.metrics {
    grid-template-columns: repeat(4, minmax(0, 1fr));
  }

  :global(.key-value-grid > div) {
    min-width: 0;
    padding: 0.75rem;
    border-radius: var(--radius-sm);
    background: var(--color-surface-2);
  }

  :global(.key-value-grid.summary > .wide) {
    grid-column: 1 / -1;
  }

  :global(.key-value-grid.summary dt) {
    color: var(--color-text-3);
    font-size: 0.68rem;
  }

  :global(.key-value-grid.summary dd) {
    margin: 0.28rem 0 0;
    overflow-wrap: anywhere;
    color: var(--color-text-1);
    font-size: 0.76rem;
    font-weight: 650;
  }

  :global(.key-value-grid.metrics dt) {
    color: var(--color-text-3);
    font-size: 0.65rem;
  }

  :global(.key-value-grid.metrics dd) {
    margin: 0.25rem 0 0;
    font-size: 0.9rem;
    font-weight: 720;
  }

  @media (max-width: 560px) {
    .key-value-grid.metrics {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }
</style>
