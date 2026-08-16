<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    ariaLabel?: string;
    class?: string;
    tableClass?: string;
    children: Snippet;
  }

  let {
    ariaLabel,
    class: className = '',
    tableClass = '',
    children
  }: Props = $props();
</script>

<div class={`table-scroll ${className}`.trim()}>
  <table class={`data-table ${tableClass}`.trim()} aria-label={ariaLabel}>
    {@render children()}
  </table>
</div>
