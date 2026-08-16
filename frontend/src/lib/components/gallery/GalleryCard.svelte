<script lang="ts">
  import { resolve } from '$app/paths';

  import type { GalleryWork } from '$lib/api/gallery';
  import WorkThumbnail from '$lib/components/ui/WorkThumbnail.svelte';
  import {
    galleryArtistPath,
    galleryTagPath,
    galleryWorkPath
  } from '$lib/gallery-routes';

  interface Props {
    work: GalleryWork;
    coverHeight?: number;
    imagePriority?: 'high' | 'low';
    selectionMode?: boolean;
    selected?: boolean;
    onSelect?: (work: GalleryWork, selected: boolean) => void;
    onOpen?: (target: ReturnType<typeof galleryWorkPath>) => void;
  }

  let {
    work,
    coverHeight,
    imagePriority = 'low',
    selectionMode = false,
    selected = false,
    onSelect,
    onOpen
  }: Props = $props();
  let isUgoira = $derived(
    work.work_kind === 'ugoira' || work.media_kind === 'ugoira_zip'
  );

  function handleOpen(event: MouseEvent): void {
    if (onOpen && isPlainPrimaryClick(event)) {
      event.preventDefault();
      onOpen?.(galleryWorkPath(work.pixiv_work_id));
    }
  }

  function isPlainPrimaryClick(event: MouseEvent): boolean {
    return (
      event.button === 0 &&
      !event.ctrlKey &&
      !event.metaKey &&
      !event.shiftKey &&
      !event.altKey
    );
  }

  function toggleSelection(): void {
    onSelect?.(work, !selected);
  }

  function compact(value: number | null): string {
    if (value === null) return '—';
    return Intl.NumberFormat('zh-CN', {
      notation: 'compact',
      maximumFractionDigits: 1
    }).format(value);
  }
</script>

{#snippet coverVisual()}
  <WorkThumbnail
    src={work.cover_url}
    alt={work.title}
    ageRating={work.age_rating}
    loading={imagePriority === 'high' ? 'eager' : 'lazy'}
    fetchPriority={imagePriority}
    zoom
    unavailableEyebrow={`PIXIV ${work.pixiv_work_id}`}
    unavailableMessage="封面不可用"
  />
{/snippet}

{#snippet coverBadges()}
  <div class="card-badges">
    {#if work.ai_generated}<span class="work-badge ai-badge">AI</span>{/if}
    {#if isUgoira}<span class="work-badge">动图</span>{/if}
    {#if work.page_count > 1}
      <span class="work-badge">{work.page_count}P</span>
    {/if}
    {#if work.age_rating === 'r18'}
      <span class="work-badge">R-18</span>
    {:else if work.age_rating === 'r18g'}
      <span class="work-badge">R-18G</span>
    {/if}
  </div>
  {#if work.bookmarked_by_current_account}
    <span class="bookmark-mark" aria-label="已收藏">♥</span>
  {/if}
{/snippet}

<article
  class="gallery-card"
  class:has-tags={work.tags.length > 0}
  class:selection-mode={selectionMode}
  class:selected
>
  <div
    class="cover-frame"
    style:height={coverHeight ? `${coverHeight}px` : undefined}
  >
    {@render coverVisual()}
    {#if selectionMode}
      <button
        class="cover-link selection-trigger"
        type="button"
        aria-label={`切换选择${work.title}`}
        onclick={toggleSelection}
      ></button>
    {:else}
      <a
        class="cover-link"
        href={resolve(galleryWorkPath(work.pixiv_work_id))}
        aria-label={`打开${work.title}`}
        onclick={handleOpen}
      ></a>
    {/if}
    {@render coverBadges()}
    {#if selectionMode}
      <label class="selection-control">
        <input
          type="checkbox"
          checked={selected}
          aria-label={`选择${work.title}`}
          onchange={(event) => onSelect?.(work, event.currentTarget.checked)}
        />
      </label>
    {/if}
  </div>

  <div class="card-copy">
    <div class="card-title-row">
      {#if selectionMode}
        <span class="work-title">{work.title}</span>
      {:else}
        <a
          href={resolve(galleryWorkPath(work.pixiv_work_id))}
          onclick={handleOpen}>{work.title}</a
        >
      {/if}
      <span class="bookmark-count">♥ {compact(work.bookmark_count)}</span>
    </div>
    {#if selectionMode}
      <span class="artist-link">{work.artist_name}</span>
    {:else}
      <a
        class="artist-link"
        href={resolve(galleryArtistPath(work.pixiv_artist_id))}
        >{work.artist_name}</a
      >
    {/if}
    {#if work.tags.length > 0}
      <div class="tag-line">
        {#each work.tags.slice(0, 3) as tag (tag.id)}
          {#if selectionMode}
            <span>#{tag.translation ?? tag.original}</span>
          {:else}
            <a href={resolve(galleryTagPath(tag.original))}
              >#{tag.translation ?? tag.original}</a
            >
          {/if}
        {/each}
      </div>
    {/if}
  </div>
</article>

<style>
  .gallery-card {
    display: grid;
    grid-template-rows: auto auto;
    gap: 10px;
    overflow: hidden;
  }

  .gallery-card.selected .cover-frame {
    outline: 3px solid var(--color-primary);
    outline-offset: -3px;
  }

  .cover-frame {
    position: relative;
    overflow: hidden;
    border-radius: var(--radius-md);
    background: var(--color-surface-2);
  }

  .cover-link {
    position: absolute;
    z-index: 2;
    inset: 0;
    display: block;
  }

  .selection-trigger {
    background: transparent;
    color: inherit;
    cursor: pointer;
  }

  .gallery-card.selection-mode :global(.thumbnail-unavailable) {
    pointer-events: none;
  }

  .card-badges {
    position: absolute;
    top: 0.55rem;
    left: 0.55rem;
    display: flex;
    gap: 0.35rem;
    pointer-events: none;
    z-index: 3;
  }

  .ai-badge {
    background: color-mix(in srgb, var(--color-primary) 88%, #202b55);
  }

  .selection-control {
    position: absolute;
    z-index: 5;
    top: 0.55rem;
    right: 0.55rem;
    display: grid;
    width: 30px;
    height: 30px;
    place-content: center;
    border-radius: 50%;
    background: rgba(0, 0, 0, 0.68);
    backdrop-filter: blur(10px);
  }

  .selection-control input {
    width: 17px;
    height: 17px;
    accent-color: var(--color-primary);
  }

  .bookmark-mark {
    position: absolute;
    right: 0.55rem;
    bottom: 0.55rem;
    display: grid;
    width: 30px;
    height: 30px;
    place-content: center;
    border-radius: 50%;
    background: rgba(0, 0, 0, 0.64);
    color: #ff5d78;
    font-size: 0.9rem;
    pointer-events: none;
    z-index: 3;
    backdrop-filter: blur(10px);
  }

  .cover-frame:has(.cover-link:hover) :global(.work-thumbnail.zoom img) {
    transform: scale(1.025);
  }

  .card-copy {
    display: grid;
    height: 38px;
    min-height: 0;
    align-content: start;
    gap: 4px;
    padding: 0 0.12rem;
  }

  .gallery-card.has-tags .card-copy {
    height: 56px;
  }

  .card-title-row {
    display: flex;
    min-width: 0;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
  }

  .card-title-row a,
  .work-title {
    overflow: hidden;
    color: var(--color-text-1);
    font-size: 0.82rem;
    font-weight: 700;
    line-height: 16px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .bookmark-count {
    flex: 0 0 auto;
    color: var(--color-text-3);
    font-size: 0.65rem;
  }

  .artist-link,
  .tag-line a,
  .tag-line span {
    color: var(--color-text-3);
    font-size: 0.7rem;
    line-height: 14px;
  }

  a.artist-link:hover,
  .tag-line a:hover {
    color: var(--color-primary);
  }

  .tag-line {
    display: flex;
    min-width: 0;
    gap: 0.38rem;
    overflow: hidden;
    white-space: nowrap;
  }
</style>
