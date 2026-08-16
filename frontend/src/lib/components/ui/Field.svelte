<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    label: string;
    description?: string;
    labelHidden?: boolean;
    wide?: boolean;
    class?: string;
    children: Snippet;
  }

  let {
    label,
    description,
    labelHidden = false,
    wide = false,
    class: className = '',
    children
  }: Props = $props();
</script>

<label class={`field${wide ? ' wide' : ''} ${className}`.trim()}>
  <span class:sr-only={labelHidden}>{label}</span>
  {@render children()}
  {#if description}<small>{description}</small>{/if}
</label>

<style>
  small {
    color: var(--color-text-3);
    font-size: 0.7rem;
    font-weight: 450;
    line-height: 1.5;
  }
</style>
