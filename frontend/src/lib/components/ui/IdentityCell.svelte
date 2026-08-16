<script lang="ts">
  import AccountAvatar from './AccountAvatar.svelte';

  interface Props {
    src?: string | null;
    title?: string;
    subtitle: string;
    size?: number;
    alt?: string;
    fallbackText?: string;
    layout?: 'inline' | 'stacked';
    class?: string;
  }

  let {
    src = null,
    title,
    subtitle,
    size = 32,
    alt = '',
    fallbackText = '',
    layout = 'inline',
    class: className = ''
  }: Props = $props();
</script>

<div
  class={`identity-cell ${layout} ${className}`.trim()}
  style={`--identity-avatar-size: ${size}px`}
>
  <AccountAvatar {src} {size} {alt} {fallbackText} />
  <div class="identity-copy">
    {#if title}<strong>{title}</strong>{/if}
    <span>{subtitle}</span>
  </div>
</div>

<style>
  .identity-cell {
    min-width: 0;
    align-items: center;
  }

  .identity-cell.inline {
    display: flex;
    gap: 0.5rem;
    white-space: nowrap;
  }

  .identity-cell.stacked {
    display: grid;
    grid-template-columns: var(--identity-avatar-size) minmax(0, 1fr);
    gap: 0.75rem;
  }

  .identity-copy {
    display: grid;
    min-width: 0;
    gap: 0.18rem;
  }

  .inline .identity-copy {
    display: block;
  }

  strong {
    overflow: hidden;
    font-size: 0.82rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  span {
    color: var(--color-text-3);
    font-size: 0.72rem;
  }

  .inline span {
    font-size: 0.7rem;
  }
</style>
