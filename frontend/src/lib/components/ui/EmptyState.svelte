<script lang="ts">
  import type { Snippet } from 'svelte';

  type EmptyStateVariant = 'panel' | 'gallery' | 'editor' | 'trace';

  interface Props {
    message?: string;
    variant?: EmptyStateVariant;
    loading?: boolean;
    ariaLive?: 'off' | 'polite' | 'assertive';
    role?: string;
    actions?: Snippet;
    children?: Snippet;
    class?: string;
  }

  let {
    message = '',
    variant = 'panel',
    loading = false,
    ariaLive,
    role,
    actions,
    children,
    class: className = ''
  }: Props = $props();

  let stateClass = $derived(
    variant === 'trace' ? 'trace-empty' : `empty-${variant}`
  );
  let live = $derived(ariaLive ?? (loading ? 'polite' : undefined));
</script>

<div class={`${stateClass} ${className}`.trim()} aria-live={live} {role}>
  {#if children}{@render children()}{:else}{message}{/if}
  {#if actions}<div class="empty-actions">{@render actions()}</div>{/if}
</div>

<style>
  .empty-panel {
    display: grid;
    min-height: 210px;
    place-content: center;
    padding: 1.5rem;
    color: var(--color-text-3);
    font-size: 0.78rem;
    text-align: center;
  }

  .empty-gallery {
    display: grid;
    min-height: 420px;
    place-content: center;
    color: var(--color-text-3);
    font-size: 0.8rem;
  }

  .empty-editor {
    display: grid;
    place-items: center;
    color: var(--color-text-3);
    font-size: 0.8rem;
  }

  .trace-empty {
    display: grid;
    min-height: 280px;
    place-content: center;
    padding: 1rem;
    text-align: center;
  }

  .trace-empty :global(strong) {
    font-size: 0.82rem;
  }

  .empty-actions {
    display: flex;
    justify-content: center;
    gap: 0.55rem;
    margin-top: 0.75rem;
  }
</style>
