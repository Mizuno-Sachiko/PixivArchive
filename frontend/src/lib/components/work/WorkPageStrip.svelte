<script lang="ts">
  import type { GalleryWorkDetail } from '$lib/api/gallery';
  import WorkThumbnail from '$lib/components/ui/WorkThumbnail.svelte';

  interface Props {
    pages: GalleryWorkDetail['pages'];
    activePageIndex: number;
    ageRating: GalleryWorkDetail['work']['age_rating'];
    onSelect: (index: number) => void;
  }

  let { pages, activePageIndex, ageRating, onSelect }: Props = $props();
  let strip = $state<HTMLDivElement>();

  function handleWheel(event: WheelEvent): void {
    if (!strip || strip.scrollWidth <= strip.clientWidth) return;
    const distance = event.deltaY || event.deltaX;
    if (distance === 0) return;
    event.preventDefault();
    strip.scrollLeft += distance;
  }
</script>

<div
  class="page-strip"
  aria-label="作品页"
  bind:this={strip}
  onwheel={handleWheel}
>
  {#each pages as workPage, index (workPage.id)}
    <button
      class:active={index === activePageIndex}
      type="button"
      aria-label={`查看第${index + 1}页`}
      aria-pressed={index === activePageIndex}
      onclick={() => onSelect(index)}
    >
      <WorkThumbnail
        src={workPage.current_media?.derivatives[0]?.url}
        alt={`第${index + 1}页`}
        {ageRating}
        fit="contain"
      />
    </button>
  {/each}
</div>

<style>
  .page-strip {
    display: flex;
    gap: 0.5rem;
    overflow-x: auto;
    overflow-y: hidden;
    overscroll-behavior-x: contain;
  }

  button {
    display: grid;
    width: 74px;
    height: 74px;
    flex: 0 0 auto;
    place-items: center;
    overflow: hidden;
    border-radius: var(--radius-sm);
    background: var(--color-surface-2);
    color: var(--color-text-3);
  }

  button.active {
    outline: 2px solid var(--color-primary);
    outline-offset: -2px;
  }
</style>
