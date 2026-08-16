<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    kind: 'theme' | 'account' | 'navigation';
    children: Snippet;
  }

  let { kind, children }: Props = $props();
</script>

<div class="topbar-popover glass-surface" data-topbar-popover={kind}>
  {@render children()}
</div>

<style>
  .topbar-popover {
    position: absolute;
    z-index: 60;
    top: calc(100% + 10px);
    right: 0;
    min-width: 190px;
    padding: 0.45rem;
    border-radius: var(--radius-md);
    background: var(--color-glass-strong);
    color: var(--color-text-1);
  }

  .topbar-popover[data-topbar-popover='navigation'] {
    position: fixed;
    top: calc(var(--topbar-height) + 8px);
    right: 12px;
    left: 12px;
    min-width: 0;
  }

  @media (max-width: 480px) {
    .topbar-popover {
      position: fixed;
      top: calc(var(--topbar-height) + 8px);
      right: 12px;
      left: 12px;
    }
  }
</style>
