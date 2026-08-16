<script lang="ts">
  interface Props {
    message?: string;
    eyebrow?: string;
    overlay?: boolean;
    onRetry?: () => void;
  }

  let {
    message = '缩略图不可用',
    eyebrow,
    overlay = false,
    onRetry
  }: Props = $props();
</script>

<div
  class:overlay
  class="thumbnail-unavailable"
  role={onRetry ? 'status' : undefined}
>
  {#if eyebrow}<span>{eyebrow}</span>{/if}
  <strong>{message}</strong>
  {#if onRetry}
    <button type="button" onclick={onRetry}>重新加载</button>
  {/if}
</div>

<style>
  .thumbnail-unavailable {
    display: grid;
    width: 100%;
    height: 100%;
    place-content: center;
    gap: 0.3rem;
    padding: 1rem;
    background:
      radial-gradient(
        circle at 20% 20%,
        var(--color-primary-soft),
        transparent 45%
      ),
      var(--color-surface-2);
    color: var(--color-text-3);
    text-align: center;
  }

  .thumbnail-unavailable.overlay {
    position: absolute;
    z-index: 4;
    inset: 0;
    background: color-mix(in srgb, var(--color-surface-2) 92%, transparent);
    color: var(--color-text-2);
    backdrop-filter: blur(10px);
  }

  span {
    font: 0.63rem var(--font-mono);
  }

  strong {
    color: var(--color-text-2);
    font-size: 0.82rem;
  }

  button {
    height: 32px;
    margin-top: 0.25rem;
    padding: 0 0.75rem;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    background: var(--color-surface-1);
    color: var(--color-primary);
    font-size: 0.72rem;
    font-weight: 700;
  }
</style>
