<script lang="ts">
  import type { PixivAgeRating } from '$lib/api/gallery';
  import { shouldMaskThumbnail } from '$lib/content-rating';
  import { contentSettingsStore } from '$lib/stores/content-settings.svelte';

  import ThumbnailUnavailable from './ThumbnailUnavailable.svelte';

  interface Props {
    src?: string | null;
    alt?: string;
    ageRating?: PixivAgeRating | null;
    maskNonAllAge?: boolean;
    loading?: 'eager' | 'lazy';
    fetchPriority?: 'high' | 'low' | 'auto';
    fit?: 'cover' | 'contain';
    zoom?: boolean;
    unavailableEyebrow?: string;
    unavailableMessage?: string;
  }

  let {
    src = null,
    alt = '',
    ageRating = null,
    maskNonAllAge,
    loading = 'lazy',
    fetchPriority = 'auto',
    fit = 'cover',
    zoom = false,
    unavailableEyebrow,
    unavailableMessage = '缩略图不可用'
  }: Props = $props();
  let masked = $derived(
    Boolean(src) &&
      shouldMaskThumbnail(
        ageRating,
        maskNonAllAge ??
          contentSettingsStore.effective.mask_non_all_age_thumbnails
      )
  );
  let observedSource = $state<string | null>();
  let observedMask = $state(false);
  let loaded = $state(false);
  let failed = $state(false);
  let retry = $state(0);
  let requestedSource = $derived(
    !masked && src
      ? retry > 0
        ? `${src}${src.includes('?') ? '&' : '?'}retry=${retry}`
        : src
      : null
  );
  let thumbnailState = $derived(
    masked
      ? 'masked'
      : !requestedSource
        ? 'unavailable'
        : failed
          ? 'failed'
          : loaded
            ? 'loaded'
            : 'loading'
  );

  $effect(() => {
    if (src === observedSource && masked === observedMask) return;
    observedSource = src;
    observedMask = masked;
    loaded = false;
    failed = false;
    retry = 0;
  });

  function handleError(): void {
    loaded = false;
    if (retry === 0) {
      retry = 1;
      return;
    }
    failed = true;
  }
</script>

<span
  class="work-thumbnail"
  class:contain={fit === 'contain'}
  class:zoom
  data-thumbnail-state={thumbnailState}
>
  {#if masked}
    <ThumbnailUnavailable eyebrow="非全年龄内容" message="缩略图已遮挡" />
  {:else if requestedSource && !failed}
    <img
      class:loaded
      src={requestedSource}
      {alt}
      {loading}
      fetchpriority={fetchPriority}
      decoding="async"
      onload={() => {
        loaded = true;
        failed = false;
      }}
      onerror={handleError}
    />
    {#if !loaded}<span class="thumbnail-loading" aria-hidden="true"></span>{/if}
  {:else}
    <ThumbnailUnavailable
      eyebrow={unavailableEyebrow}
      message={failed ? '缩略图未加载' : unavailableMessage}
    />
  {/if}
</span>

<style>
  .work-thumbnail,
  img,
  .thumbnail-loading {
    display: block;
    width: 100%;
    height: 100%;
  }

  .work-thumbnail {
    position: relative;
    overflow: hidden;
  }

  img {
    opacity: 0;
    object-fit: cover;
    transition:
      opacity var(--motion-normal) var(--ease-standard),
      transform var(--motion-slow) var(--ease-standard);
  }

  .contain img {
    object-fit: contain;
  }

  img.loaded {
    opacity: 1;
  }

  .thumbnail-loading {
    position: absolute;
    inset: 0;
    overflow: hidden;
    background: var(--color-surface-2);
  }

  .thumbnail-loading::after {
    position: absolute;
    inset: 0;
    background: linear-gradient(
      105deg,
      transparent 28%,
      color-mix(in srgb, var(--color-primary-soft) 70%, transparent) 48%,
      transparent 68%
    );
    content: '';
    transform: translateX(-100%);
    animation: thumbnail-loading 1.3s ease-in-out infinite;
  }

  @keyframes thumbnail-loading {
    to {
      transform: translateX(100%);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    img {
      transition: none;
    }

    .thumbnail-loading::after {
      animation: none;
    }
  }
</style>
