<script lang="ts">
  import Icon from './Icon.svelte';

  interface Props {
    src?: string | null;
    size?: number;
    alt?: string;
    fallbackText?: string;
  }

  let { src = null, size = 32, alt = '', fallbackText = '' }: Props = $props();
  let failed = $state(false);
  let observedSource = $state('');

  $effect(() => {
    const next = src ?? '';
    if (next === observedSource) return;
    observedSource = next;
    failed = false;
  });
</script>

<span
  class="account-avatar"
  style={`--account-avatar-size: ${size}px`}
  aria-hidden={alt ? undefined : 'true'}
>
  {#if src && !failed}
    <img {src} {alt} onerror={() => (failed = true)} />
  {:else}
    {#if fallbackText.trim()}
      <span class="fallback-text" aria-hidden="true">
        {fallbackText.trim().slice(0, 1)}
      </span>
    {:else}
      <Icon name="user" size={Math.max(14, Math.round(size * 0.48))} />
    {/if}
  {/if}
</span>

<style>
  .account-avatar {
    display: grid;
    width: var(--account-avatar-size);
    height: var(--account-avatar-size);
    min-width: var(--account-avatar-size);
    flex: 0 0 var(--account-avatar-size);
    overflow: hidden;
    place-items: center;
    border-radius: 50%;
    background: var(--color-primary-soft);
    color: var(--color-primary);
  }

  img {
    display: block;
    width: 100%;
    height: 100%;
    min-width: 0;
    min-height: 0;
    object-fit: cover;
  }

  .fallback-text {
    font-size: max(0.75rem, calc(var(--account-avatar-size) * 0.34));
    font-weight: 760;
    line-height: 1;
  }
</style>
