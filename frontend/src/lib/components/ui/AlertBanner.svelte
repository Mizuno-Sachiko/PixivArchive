<script lang="ts">
  import type { Snippet } from 'svelte';

  import Icon from './Icon.svelte';

  interface Props {
    title: string;
    message: string;
    tone?: 'warning' | 'error';
    actions?: Snippet;
  }

  let { title, message, tone = 'warning', actions }: Props = $props();
</script>

<aside class="alert-banner {tone}" aria-live="polite">
  <span class="icon"><Icon name="alert" size={19} /></span>
  <div class="alert-copy">
    <strong>{title}</strong>
    <p>{message}</p>
  </div>
  {#if actions}
    <div class="alert-actions">{@render actions()}</div>
  {/if}
</aside>

<style>
  .alert-banner {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 0.8rem;
    align-items: center;
    padding: 0.9rem 1rem;
    border: 1px solid color-mix(in srgb, var(--color-warning) 32%, transparent);
    border-radius: var(--radius-md);
    background: var(--color-warning-soft);
    color: var(--color-text-1);
  }

  .alert-banner.error {
    border-color: color-mix(in srgb, var(--color-error) 32%, transparent);
    background: var(--color-error-soft);
  }

  .icon {
    display: grid;
    color: var(--color-warning);
  }

  .alert-copy {
    min-width: 0;
  }

  .error .icon {
    color: var(--color-error);
  }

  strong {
    display: block;
    font-size: 0.88rem;
  }

  p {
    margin: 0.2rem 0 0;
    color: var(--color-text-2);
    font-size: 0.83rem;
    line-height: 1.5;
  }

  .alert-actions {
    display: flex;
    grid-column: 2;
    flex-wrap: wrap;
    gap: 0.55rem;
  }
</style>
