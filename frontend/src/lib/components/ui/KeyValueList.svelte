<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    children: Snippet;
    variant?: 'rows' | 'split';
    labelWidth?: string;
    class?: string;
  }

  let {
    children,
    variant = 'rows',
    labelWidth = '100px',
    class: className = ''
  }: Props = $props();
</script>

<dl
  class={`key-value-list ${variant} ${className}`.trim()}
  style={`--key-value-label-width: ${labelWidth}`}
>
  {@render children()}
</dl>

<style>
  .key-value-list {
    display: grid;
    margin: 0;
  }

  .key-value-list.rows {
    gap: 0;
  }

  :global(.key-value-list.rows > div) {
    display: grid;
    grid-template-columns: var(--key-value-label-width) minmax(0, 1fr);
    gap: 0.7rem;
    padding: 0.5rem 0;
    border-bottom: 1px solid var(--color-border);
  }

  :global(.key-value-list.rows dt) {
    color: var(--color-text-3);
    font-size: 0.69rem;
  }

  :global(.key-value-list.rows dd) {
    min-width: 0;
    margin: 0;
    overflow-wrap: anywhere;
    color: var(--color-text-2);
    font-size: 0.72rem;
  }

  .key-value-list.split {
    gap: 0.15rem;
  }

  :global(.key-value-list.split > div) {
    display: flex;
    justify-content: space-between;
    gap: 1rem;
    padding: 0.65rem 0;
    border-bottom: 1px solid var(--color-border);
  }

  :global(.key-value-list.split dt) {
    color: var(--color-text-3);
    font-size: 0.73rem;
  }

  :global(.key-value-list.split dd) {
    margin: 0;
    font-size: 0.78rem;
    font-weight: 700;
  }
</style>
