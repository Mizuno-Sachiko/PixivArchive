<script lang="ts">
  import { innerHeight, scrollY } from 'svelte/reactivity/window';

  import type { GalleryWork } from '$lib/api/gallery';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import type { galleryWorkPath } from '$lib/gallery-routes';

  import GalleryCard from './GalleryCard.svelte';
  import ViewportPager from './ViewportPager.svelte';
  import {
    balancedPlacements,
    galleryCardChromeHeight,
    responsiveColumnCount,
    visiblePlacements
  } from './waterfall';

  interface Props {
    items: GalleryWork[];
    loading?: boolean;
    hasMore?: boolean;
    loadEnabled?: boolean;
    selectionMode?: boolean;
    selectedIds?: Set<string>;
    onLoadMore?: () => void;
    onSelect?: (work: GalleryWork, selected: boolean) => void;
    onOpen?: (target: ReturnType<typeof galleryWorkPath>) => void;
  }

  let {
    items,
    loading = false,
    hasMore = false,
    loadEnabled = true,
    selectionMode = false,
    selectedIds = new Set<string>(),
    onLoadMore,
    onSelect,
    onOpen
  }: Props = $props();
  let waterfall = $state<HTMLDivElement>();
  let width = $state(280);
  let waterfallTop = $state(0);
  const gap = 16;
  let columnCount = $derived(responsiveColumnCount(width, 220, gap));
  let columnWidth = $derived(
    Math.max(120, (width - gap * (columnCount - 1)) / columnCount)
  );
  let layout = $derived(
    balancedPlacements(
      items.map((work) => ({
        id: work.id,
        width: work.cover_width ?? 640,
        height: work.cover_height ?? 900,
        extraHeight: galleryCardChromeHeight(work.tags.length > 0)
      })),
      { columnCount, columnWidth, gap }
    )
  );
  let renderedPlacements = $derived(
    visiblePlacements(layout.placements, {
      scrollTop: Math.max(0, (scrollY.current ?? 0) - waterfallTop),
      viewportHeight: innerHeight.current ?? 900,
      overscan: 1800
    })
  );

  function imagePriority(
    placement: (typeof layout.placements)[number]
  ): 'high' | 'low' {
    const viewportTop = Math.max(0, (scrollY.current ?? 0) - waterfallTop);
    const viewportBottom = viewportTop + (innerHeight.current ?? 900);
    return placement.top + placement.outerHeight >= viewportTop - 160 &&
      placement.top <= viewportBottom + 160
      ? 'high'
      : 'low';
  }

  $effect(() => {
    const element = waterfall;
    if (!element) return;
    let frame = 0;
    const updateWaterfallTop = () => {
      waterfallTop =
        element.getBoundingClientRect().top + (scrollY.current ?? 0);
    };
    const scheduleUpdate = () => {
      if (frame) return;
      frame = requestAnimationFrame(() => {
        frame = 0;
        updateWaterfallTop();
      });
    };
    const observer = new ResizeObserver(scheduleUpdate);
    observer.observe(document.documentElement);
    if (document.body) observer.observe(document.body);
    let ancestor = element.parentElement;
    while (ancestor) {
      observer.observe(ancestor);
      ancestor = ancestor.parentElement;
    }
    scheduleUpdate();
    window.addEventListener('resize', scheduleUpdate, { passive: true });
    window.addEventListener('scroll', scheduleUpdate, { passive: true });
    return () => {
      observer.disconnect();
      window.removeEventListener('resize', scheduleUpdate);
      window.removeEventListener('scroll', scheduleUpdate);
      if (frame) cancelAnimationFrame(frame);
    };
  });
</script>

{#if items.length === 0 && !loading}
  <EmptyState message="没有符合条件的作品" variant="gallery" />
{:else}
  <div
    class="waterfall"
    bind:this={waterfall}
    bind:clientWidth={width}
    aria-label="作品瀑布流"
  >
    <div class="waterfall-canvas" style:height={`${layout.totalHeight}px`}>
      {#each renderedPlacements as placement (placement.id)}
        {@const work = items[placement.index]}
        <div
          class="card-slot"
          data-gallery-anchor={placement.id}
          style:left={`${placement.left}px`}
          style:top={`${placement.top}px`}
          style:width={`${placement.width}px`}
          style:height={`${placement.outerHeight}px`}
        >
          <GalleryCard
            {work}
            coverHeight={placement.height}
            imagePriority={imagePriority(placement)}
            {selectionMode}
            selected={selectedIds.has(work.id)}
            {onSelect}
            {onOpen}
          />
        </div>
      {/each}
    </div>
    {#if onLoadMore}
      <ViewportPager
        enabled={loadEnabled}
        {hasMore}
        {loading}
        onLoadMore={() => onLoadMore?.()}
      />
    {/if}
    {#if loading}
      <div class="gallery-loading" aria-live="polite">正在读取作品…</div>
    {/if}
  </div>
{/if}

<style>
  .waterfall,
  .waterfall-canvas {
    position: relative;
    width: 100%;
    min-width: 0;
  }

  .card-slot {
    position: absolute;
  }

  .gallery-loading {
    width: max-content;
    padding: 0.5rem 0.8rem;
    margin: 0.8rem auto 0;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-pill);
    background: var(--color-glass);
    color: var(--color-text-2);
    font-size: 0.72rem;
    backdrop-filter: blur(16px);
  }
</style>
