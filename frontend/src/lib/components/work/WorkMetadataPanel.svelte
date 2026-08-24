<script lang="ts">
  import { resolve } from '$app/paths';

  import {
    workDownloadUrl,
    type GalleryWorkDetail,
    type WorkRevisionSummary
  } from '$lib/api/gallery';
  import type { PixivAccount } from '$lib/api/system';
  import KeyValueGrid from '$lib/components/ui/KeyValueGrid.svelte';
  import PixivSourceLink from '$lib/components/ui/PixivSourceLink.svelte';
  import ReadableTime from '$lib/components/ui/ReadableTime.svelte';
  import { formatBytes } from '$lib/format';
  import { galleryArtistPath, galleryTagPath } from '$lib/gallery-routes';

  import PixivDescription from './PixivDescription.svelte';
  import WorkActions from './WorkActions.svelte';

  interface Props {
    detail: GalleryWorkDetail;
    revisions: WorkRevisionSummary[];
    account: PixivAccount | null;
    bookmarkDisabled: boolean;
    busy: string;
    notice: string;
    error: string;
    onToggleBookmark: () => void;
    onMoveToTrash: () => void;
    onRestoreFromTrash: () => void;
    requestPurge: (returnFocus: HTMLElement) => void;
  }

  let {
    detail,
    revisions,
    account,
    bookmarkDisabled,
    busy,
    notice,
    error,
    onToggleBookmark,
    onMoveToTrash,
    onRestoreFromTrash,
    requestPurge
  }: Props = $props();
</script>

<section class="work-copy">
  <header>
    <div class="title-line">
      <h1>{detail.work.title}</h1>
      <div class="title-actions">
        {#if detail.ugoira}<span class="work-badge">动图</span>{/if}
        <a
          class="download-link"
          href={workDownloadUrl(detail.work.id)}
          rel="external"
          download
          title="下载全部已归档原图">下载全部</a
        >
        <PixivSourceLink
          href={`https://www.pixiv.net/artworks/${detail.work.pixiv_work_id}`}
          label="在Pixiv打开"
        />
      </div>
    </div>
    <a
      class="artist-name"
      href={resolve(galleryArtistPath(detail.work.pixiv_artist_id))}
      >{detail.work.artist_name}</a
    >
    <div class="pixiv-work-id">
      <span>Pixiv ID</span>
      <strong>{detail.work.pixiv_work_id}</strong>
    </div>
    {#if detail.work.pixiv_published_at}
      <div class="published-time">
        <span>发布时间</span>
        <ReadableTime value={detail.work.pixiv_published_at} exact />
      </div>
    {/if}
  </header>

  <WorkActions
    trashed={detail.work.collection_state === 'trash'}
    canRestore={detail.trash_capabilities?.can_restore ?? false}
    canBookmark={Boolean(account?.bookmark_writeback_enabled)}
    {bookmarkDisabled}
    bookmarked={detail.work.bookmarked_by_current_account}
    {busy}
    {notice}
    {error}
    {onToggleBookmark}
    {onMoveToTrash}
    onRestore={onRestoreFromTrash}
    onPurge={requestPurge}
  />

  {#if detail.work.description}
    <PixivDescription value={detail.work.description} />
  {/if}

  <div class="tag-cloud">
    {#each detail.work.tags as tag (tag.id)}
      <a href={resolve(galleryTagPath(tag.original))}
        >#{tag.translation ?? tag.original}</a
      >
    {/each}
  </div>

  <KeyValueGrid variant="metrics">
    <div>
      <dt>收藏</dt>
      <dd>{detail.work.bookmark_count ?? '—'}</dd>
    </div>
    <div>
      <dt>浏览</dt>
      <dd>{detail.work.view_count ?? '—'}</dd>
    </div>
    <div>
      <dt>喜欢</dt>
      <dd>{detail.work.like_count ?? '—'}</dd>
    </div>
    <div>
      <dt>页数</dt>
      <dd>{detail.work.page_count}</dd>
    </div>
  </KeyValueGrid>

  <section class="detail-section">
    <h2>媒体</h2>
    {#each detail.pages as workPage (workPage.id)}
      <div class="media-row">
        <span>第{workPage.page_index + 1}页</span>
        {#if workPage.current_media}
          <strong>
            {workPage.current_media.format.toUpperCase()} ·
            {formatBytes(workPage.current_media.byte_size)}
          </strong>
        {:else}
          <strong>原图不可用</strong>
        {/if}
      </div>
    {/each}
  </section>

  <section class="detail-section">
    <h2>修订历史</h2>
    <div class="revision-list">
      {#each revisions as revision (revision.id)}
        <div>
          <strong>{revision.title}</strong>
          <span class="revision-time">
            <ReadableTime value={revision.captured_at} exact />
          </span>
          <div class="revision-details">
            <small class="revision-meta"
              >{revision.page_count}页 · {revision.work_kind}</small
            >
            {#if revision.sources.length > 0}
              <small class="revision-source">
                来自：{revision.sources
                  .map(
                    (source) =>
                      `${source.subscription_name} · 账户${source.pixiv_user_id}`
                  )
                  .join('、')}
              </small>
            {/if}
          </div>
        </div>
      {/each}
    </div>
  </section>
</section>

<style>
  .work-copy,
  .detail-section,
  .revision-list {
    display: grid;
    min-width: 0;
    gap: 1rem;
  }

  .work-copy {
    height: 100%;
    padding-right: 0.35rem;
    align-content: start;
    grid-auto-rows: max-content;
    overflow-x: hidden;
    overflow-y: auto;
  }

  header {
    display: grid;
    gap: 0.6rem;
  }

  .title-line {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    align-items: start;
    gap: 0.7rem;
  }

  .title-actions {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .download-link {
    display: inline-flex;
    min-height: 36px;
    align-items: center;
    padding: 0 0.8rem;
    border-radius: var(--radius-sm);
    background: var(--color-surface-2);
    color: var(--color-text-2);
    font-size: 0.7rem;
    font-weight: 700;
    white-space: nowrap;
  }

  .download-link:hover,
  .download-link:focus-visible {
    background: var(--color-primary-soft);
    color: var(--color-primary);
  }

  h1 {
    margin: 0;
    font-size: 2.4rem;
    line-height: 1.08;
    letter-spacing: 0;
    overflow-wrap: anywhere;
  }

  .artist-name {
    width: fit-content;
    max-width: 100%;
    color: var(--color-text-1);
    font-size: 0.86rem;
    font-weight: 700;
    overflow-wrap: anywhere;
  }

  .pixiv-work-id {
    display: flex;
    align-items: baseline;
    gap: 0.45rem;
    color: var(--color-text-3);
    font-size: 0.68rem;
  }

  .pixiv-work-id strong {
    color: var(--color-text-2);
    font-family: var(--font-mono);
    font-weight: 650;
  }

  .published-time {
    display: flex;
    width: fit-content;
    max-width: 100%;
    align-items: baseline;
    gap: 0.5rem;
    color: var(--color-text-3);
    font-size: 0.68rem;
  }

  .published-time > span {
    flex: 0 0 auto;
    font-weight: 650;
  }

  .tag-cloud {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.55rem;
  }

  .tag-cloud a {
    display: inline-flex;
    max-width: 100%;
    height: 30px;
    align-items: center;
    padding: 0 0.62rem;
    overflow: hidden;
    border-radius: var(--radius-pill);
    background: var(--color-primary-soft);
    color: var(--color-primary);
    font-size: 0.7rem;
    font-weight: 650;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .detail-section {
    padding-top: 1rem;
    border-top: 1px solid var(--color-border);
  }

  .detail-section h2 {
    margin: 0;
    font-size: 0.92rem;
  }

  .media-row {
    display: grid;
    min-width: 0;
    grid-template-columns: 68px minmax(0, 1fr);
    gap: 0.7rem;
    align-items: center;
    font-size: 0.7rem;
  }

  .media-row span {
    color: var(--color-text-3);
  }

  .media-row strong {
    overflow-wrap: anywhere;
  }

  .revision-list > div {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: 0.25rem 0.8rem;
    padding: 0.75rem;
    border-radius: var(--radius-sm);
    background: var(--color-surface-2);
  }

  .revision-list strong {
    font-size: 0.75rem;
  }

  .revision-list .revision-time,
  .revision-list small {
    color: var(--color-text-3);
    font-size: 0.66rem;
  }

  .revision-list .revision-time {
    justify-self: end;
    text-align: right;
  }

  .revision-details {
    display: flex;
    min-width: 0;
    grid-column: 1 / -1;
    flex-wrap: wrap;
    align-items: baseline;
    justify-content: space-between;
    gap: 0.25rem 0.8rem;
  }

  .revision-list .revision-meta,
  .revision-list .revision-source {
    flex: 0 0 auto;
    max-width: 100%;
  }

  .revision-list .revision-source {
    margin-left: auto;
    text-align: right;
  }

  .revision-list small {
    overflow-wrap: anywhere;
  }

  @media (max-width: 720px) {
    .work-copy {
      height: auto;
      padding-right: 0;
      overflow: visible;
    }

    h1 {
      font-size: 1.8rem;
    }
  }

  @media (max-width: 560px) {
    .media-row {
      grid-template-columns: 1fr;
      gap: 0.2rem;
    }
  }
</style>
