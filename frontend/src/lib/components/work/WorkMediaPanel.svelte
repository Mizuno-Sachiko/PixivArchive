<script lang="ts">
  import type { GalleryWorkDetail } from '$lib/api/gallery';
  import UgoiraCanvas from '$lib/components/viewer/UgoiraCanvas.svelte';
  import type { UgoiraDecodeLimits } from '$lib/workers/ugoira-protocol';

  import WorkPageStrip from './WorkPageStrip.svelte';

  interface Props {
    detail: GalleryWorkDetail;
    activePageIndex: number;
    viewerOpen: boolean;
    ugoiraLimits?: UgoiraDecodeLimits;
    onSelectPage: (index: number) => void;
    onOpenViewer: (returnFocus: HTMLElement) => void;
  }

  let {
    detail,
    activePageIndex,
    viewerOpen,
    ugoiraLimits,
    onSelectPage,
    onOpenViewer
  }: Props = $props();
  let activePage = $derived(detail.pages[activePageIndex] ?? null);
  let activeMedia = $derived(activePage?.current_media ?? null);
  let activeDominantColor = $derived(
    activeMedia?.derivatives[0]?.dominant_color ?? 'var(--color-viewer-bg)'
  );
  let activePageIsUgoira = $derived(
    Boolean(detail.ugoira && activeMedia?.media_kind === 'ugoira_zip')
  );

  function handlePreviewWheel(event: WheelEvent): void {
    if (detail.pages.length <= 1 || event.deltaY === 0) return;
    event.preventDefault();
    onSelectPage(activePageIndex + (event.deltaY > 0 ? 1 : -1));
  }
</script>

<section class="work-media">
  <div
    class="source-preview"
    style:--source-dominant-color={activeDominantColor}
    onwheel={handlePreviewWheel}
  >
    {#if activeMedia}
      {#if activePageIsUgoira && detail.ugoira}
        <UgoiraCanvas
          mediaRevisionId={activeMedia.id}
          manifest={detail.ugoira}
          playing={!viewerOpen}
          limits={ugoiraLimits}
        />
      {:else}
        <img
          src={activeMedia.source_url}
          alt={`${detail.work.title} 第${activePageIndex + 1}页`}
        />
      {/if}
      <button
        class="source-open"
        type="button"
        aria-label="查看原图"
        onclick={(event) => onOpenViewer(event.currentTarget)}
      ></button>
    {:else}
      <div class="source-placeholder">原图不可用</div>
    {/if}
    {#if activePageIsUgoira}<span class="work-badge">动图</span>{/if}
  </div>
  {#if detail.pages.length > 1}
    <WorkPageStrip
      pages={detail.pages}
      {activePageIndex}
      ageRating={detail.work.age_rating}
      onSelect={onSelectPage}
    />
  {/if}
</section>

<style>
  .work-media {
    display: grid;
    min-width: 0;
    height: 100%;
    min-height: 0;
    grid-template-rows: minmax(0, 1fr) auto;
    gap: 1rem;
    overflow: hidden;
  }

  .source-preview {
    position: relative;
    display: grid;
    width: 100%;
    height: 100%;
    min-height: 0;
    place-items: center;
    overflow: hidden;
    border-radius: var(--radius-lg);
    background: var(--color-surface-2);
  }

  .source-preview img {
    display: block;
    width: 100%;
    height: 100%;
    min-width: 0;
    min-height: 0;
    object-fit: contain;
  }

  :global(.source-preview .ugoira-stage) {
    width: 100%;
    height: 100%;
  }

  .source-open {
    position: absolute;
    z-index: 1;
    inset: 0;
    border-radius: inherit;
    background: transparent;
    cursor: zoom-in;
  }

  .source-open:focus-visible {
    outline: 3px solid var(--color-primary);
    outline-offset: -3px;
  }

  .source-preview .work-badge {
    position: absolute;
    z-index: 2;
    top: 0.8rem;
    left: 0.8rem;
  }

  .source-placeholder {
    color: var(--color-text-3);
    font-size: 0.8rem;
  }

  @media (max-width: 720px) {
    .work-media {
      height: auto;
      overflow: visible;
    }

    .source-preview {
      height: min(68vh, 760px);
      min-height: 360px;
    }
  }
</style>
