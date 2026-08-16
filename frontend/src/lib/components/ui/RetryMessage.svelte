<script lang="ts">
  import Button from './Button.svelte';

  interface Props {
    message: string;
    busy?: boolean;
    actionLabel?: string;
    onRetry: () => void;
  }

  let {
    message,
    busy = false,
    actionLabel = '重新读取',
    onRetry
  }: Props = $props();
</script>

<div class="retry-message" role="alert">
  <span class="inline-message error">{message}</span>
  <Button class="retry-message-button" disabled={busy} onclick={onRetry}
    >{actionLabel}</Button
  >
</div>

<style>
  .retry-message {
    display: flex;
    align-items: center;
    justify-content: space-between;
    min-width: 0;
    gap: 0.75rem;
    padding: 0.75rem 0.85rem;
    border: 1px solid color-mix(in srgb, var(--color-error) 32%, transparent);
    border-radius: var(--radius-md);
    background: var(--color-error-soft);
  }

  .inline-message {
    flex: 1;
    min-width: 0;
    line-height: 1.45;
  }

  :global(.retry-message-button) {
    flex: none;
  }

  @media (max-width: 640px) {
    .retry-message {
      align-items: flex-start;
      flex-wrap: wrap;
    }
  }
</style>
