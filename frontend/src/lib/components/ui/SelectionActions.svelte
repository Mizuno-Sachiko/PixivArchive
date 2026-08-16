<script lang="ts">
  import type { Snippet } from 'svelte';

  import Button from './Button.svelte';

  interface Props {
    label: string;
    showLabel?: boolean;
    onSelectAll: () => void;
    onInvert: () => void;
    onClear: () => void;
    onExit: () => void;
    actions?: Snippet;
  }

  let {
    label,
    showLabel = true,
    onSelectAll,
    onInvert,
    onClear,
    onExit,
    actions
  }: Props = $props();
</script>

<div
  class:without-label={!showLabel}
  class="selection-actions"
  aria-label="批量选择工具栏"
>
  {#if showLabel}<strong>{label}</strong>{/if}
  <Button onclick={onSelectAll}>全选</Button>
  <Button onclick={onInvert}>反选</Button>
  <Button onclick={onClear}>全不选</Button>
  {#if actions}{@render actions()}{/if}
  <Button onclick={onExit}>退出多选</Button>
</div>

<style>
  .selection-actions {
    display: flex;
    min-width: 0;
    flex: 1 1 auto;
    align-items: center;
    justify-content: flex-end;
    gap: 0.55rem;
  }

  .selection-actions strong {
    margin-right: 0.15rem;
    font-size: 0.78rem;
    white-space: nowrap;
  }

  .selection-actions.without-label {
    flex: 0 1 auto;
  }

  @media (max-width: 900px) {
    .selection-actions {
      flex-wrap: wrap;
      justify-content: flex-start;
    }
  }
</style>
