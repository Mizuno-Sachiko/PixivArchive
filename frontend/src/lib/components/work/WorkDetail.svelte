<script lang="ts">
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { page } from '$app/state';
  import { onMount } from 'svelte';

  import { addBookmark, removeBookmark } from '$lib/api/bookmarks';
  import {
    getWorkDetail,
    getWorkRevisions,
    resolveWorkIdByPixivId,
    type GalleryWorkDetail,
    type WorkRevisionSummary
  } from '$lib/api/gallery';
  import { systemApi } from '$lib/api/system';
  import { moveWorkToTrash, purgeWork, restoreWork } from '$lib/api/trash';
  import {
    AppEventRefreshCoordinator,
    currentAppEventVersion
  } from '$lib/app-event-refresh';
  import AlertBanner from '$lib/components/ui/AlertBanner.svelte';
  import ConfirmDialog from '$lib/components/ui/ConfirmDialog.svelte';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import RetryMessage from '$lib/components/ui/RetryMessage.svelte';
  import UnifiedViewer from '$lib/components/viewer/UnifiedViewer.svelte';
  import {
    isPixivAccountConflict,
    pixivAccountActionFailureMessage
  } from '$lib/pixiv-account-errors';
  import { pixivAccountStore } from '$lib/stores/pixiv-account.svelte';
  import {
    clearDetailSource,
    currentDetailSource,
    detailReturnRoute
  } from '$lib/stores/detail-navigation';
  import { removeWorkFromTrashReturn } from '$lib/components/trash/trash-return';
  import type { UgoiraDecodeLimits } from '$lib/workers/ugoira-protocol';

  import WorkMediaPanel from './WorkMediaPanel.svelte';
  import WorkMetadataPanel from './WorkMetadataPanel.svelte';

  interface Props {
    pixivWorkId?: number;
    workId?: string;
  }

  let { pixivWorkId, workId }: Props = $props();
  let detail = $state<GalleryWorkDetail | null>(null);
  let revisions = $state<WorkRevisionSummary[]>([]);
  let actionAccount = $derived(pixivAccountStore.currentForAction);
  let trashRetentionDays = $state(30);
  let ugoiraLimits = $state<UgoiraDecodeLimits | undefined>();
  let defaultPrivateBookmark = $state(false);
  let activePageIndex = $state(0);
  let viewerPage = $state<number | null>(null);
  let viewerReturnFocus = $state<HTMLElement | null>(null);
  let busy = $state('');
  let notice = $state('');
  let actionError = $state('');
  let loadError = $state('');
  let loadRetryable = $state(false);
  let missingWork = $state(false);
  let detailAccountId = $state<string | null>(null);
  let purgeConfirmation = $state(false);
  let purgeReturnFocus = $state<HTMLElement | null>(null);
  let loadRevision = 0;
  let currentTargetKey = detailTargetKey();
  const detailResources = [
    'work',
    'pixiv_bookmark',
    'pixiv_account',
    'system_setting'
  ] as const;
  const detailRefresh = new AppEventRefreshCoordinator(load);

  $effect(() => {
    const targetKey = detailTargetKey();
    if (targetKey !== currentTargetKey) {
      currentTargetKey = targetKey;
      loadRevision += 1;
      clearWorkView();
    }
    detailRefresh.observe(detailVersion());
  });

  $effect(() => {
    const source = currentDetailSource(page.state);
    if (!source) return;
    return () => clearDetailSource(source.key);
  });

  onMount(() => {
    detailRefresh.start(detailVersion());
    return () => {
      detailRefresh.dispose();
      loadRevision += 1;
    };
  });

  function detailVersion(): string {
    return `${pixivWorkId ?? ''}:${workId ?? ''}:${accountStateVersion()}:${currentAppEventVersion(detailResources)}`;
  }

  function accountStateVersion(): string {
    const current = pixivAccountStore.current;
    return current
      ? `${current.account_id}:${current.state}:${current.revision ?? ''}`
      : '';
  }

  function detailTargetKey(): string {
    return `${pixivWorkId ?? ''}:${workId ?? ''}`;
  }

  async function load(): Promise<boolean> {
    const revision = ++loadRevision;
    const requestedPixivWorkId = pixivWorkId;
    const requestedWorkId = workId;
    const expectedAccountId = pixivAccountStore.current?.account_id ?? null;
    const expectedAccountVersion = accountStateVersion();
    const targetKey = detailTargetKey();
    if (detail && detailAccountId !== expectedAccountId) {
      clearWorkView();
    }
    loadError = '';
    loadRetryable = false;
    try {
      const resolvedWorkId = await resolveWorkId(
        requestedPixivWorkId,
        requestedWorkId
      );
      if (!isCurrentLoad(revision, expectedAccountVersion, targetKey)) {
        return false;
      }
      if (!resolvedWorkId) {
        clearWorkView();
        missingWork = true;
        return true;
      }
      const [workDetail, revisionItems, settings] = await Promise.all([
        getWorkDetail(resolvedWorkId),
        getWorkRevisions(resolvedWorkId),
        systemApi.settings()
      ]);
      if (!isCurrentLoad(revision, expectedAccountVersion, targetKey)) {
        return false;
      }
      const sameWork = detail?.work.id === workDetail.work.id;
      missingWork = false;
      detail = workDetail;
      detailAccountId = expectedAccountId;
      revisions = revisionItems;
      if (sameWork) {
        preserveVisiblePage(workDetail);
      } else {
        resetPageSelection();
      }
      trashRetentionDays = settings.storage.trash_retention_days;
      defaultPrivateBookmark = settings.pixiv.default_private_bookmark;
      ugoiraLimits = undefined;
      if (settings.ugoira) {
        const maximumZipBytes = settings.ugoira.max_zip_bytes;
        const maximumPixels = settings.ugoira.max_pixels_per_frame;
        ugoiraLimits = {
          maximumCompressedBytes: maximumZipBytes,
          maximumFrameCount: settings.ugoira.max_frames,
          maximumEntryBytes: Math.min(maximumZipBytes, maximumPixels * 4),
          maximumExpandedBytes: Math.min(
            Number.MAX_SAFE_INTEGER,
            maximumZipBytes * 40
          ),
          maximumPixelsPerFrame: maximumPixels,
          decodedCacheBytes: settings.ugoira.decoded_frame_cache_bytes
        };
      }
      return true;
    } catch {
      if (isCurrentLoad(revision, expectedAccountVersion, targetKey)) {
        if (!detail && missingWork) return false;
        loadRetryable = true;
        loadError = detail
          ? '作品详情更新失败，当前仍显示上次读取的数据'
          : '作品详情暂时无法读取';
      }
      return false;
    }
  }

  function clearWorkView(): void {
    detail = null;
    revisions = [];
    resetPageSelection();
    busy = '';
    notice = '';
    actionError = '';
    loadError = '';
    loadRetryable = false;
    missingWork = false;
    detailAccountId = null;
    purgeConfirmation = false;
    purgeReturnFocus = null;
    ugoiraLimits = undefined;
  }

  function resetPageSelection(): void {
    activePageIndex = 0;
    viewerPage = null;
    viewerReturnFocus = null;
  }

  function preserveVisiblePage(updated: GalleryWorkDetail): void {
    const lastPageIndex = updated.pages.length - 1;
    activePageIndex = Math.min(activePageIndex, lastPageIndex);
    if (viewerPage === null) return;
    const preservedViewerPage = Math.min(viewerPage, lastPageIndex);
    viewerPage = updated.pages[preservedViewerPage]?.current_media
      ? preservedViewerPage
      : null;
  }

  async function resolveWorkId(
    requestedPixivWorkId: number | undefined,
    requestedWorkId: string | undefined
  ): Promise<string | null> {
    if (requestedWorkId) return requestedWorkId;
    if (!requestedPixivWorkId) return null;
    return resolveWorkIdByPixivId(requestedPixivWorkId);
  }

  async function toggleBookmark(): Promise<void> {
    const accountId = actionAccount?.account_id;
    if (!detail || !accountId || detailAccountId !== accountId) return;
    const current = detail.work;
    busy = 'bookmark';
    notice = '';
    actionError = '';
    try {
      const result = current.bookmarked_by_current_account
        ? await removeBookmark(current.pixiv_work_id, accountId)
        : await addBookmark({
            account_id: accountId,
            work_id: current.pixiv_work_id,
            visibility: defaultPrivateBookmark ? 'private' : 'public',
            tags: []
          });
      if (!isCurrentBookmarkAction(current.id, accountId)) return;
      if (result.status !== 'succeeded') {
        throw new Error(result.error_class ?? 'bookmark_failed');
      }
      const visibleDetail = detail;
      if (!visibleDetail) return;
      detail = {
        ...visibleDetail,
        work: {
          ...visibleDetail.work,
          bookmarked_by_current_account: !current.bookmarked_by_current_account,
          bookmark_id: result.bookmark_id
        }
      };
      notice = !current.bookmarked_by_current_account
        ? '已同步到Pixiv收藏'
        : '已从Pixiv收藏中移除';
    } catch (cause) {
      if (isPixivAccountConflict(cause)) void pixivAccountStore.load();
      if (isCurrentBookmarkAction(current.id, accountId)) {
        actionError = pixivAccountActionFailureMessage(
          cause,
          'Pixiv收藏同步失败'
        );
      }
    } finally {
      if (isCurrentWork(current.id) && busy === 'bookmark') busy = '';
    }
  }

  function isCurrentWork(id: string): boolean {
    return detail?.work.id === id;
  }

  function isCurrentBookmarkAction(workId: string, accountId: string): boolean {
    return (
      isCurrentWork(workId) &&
      pixivAccountStore.currentForAction?.account_id === accountId
    );
  }

  function isCurrentLoad(
    revision: number,
    expectedAccountVersion: string,
    targetKey: string
  ): boolean {
    return (
      loadRevision === revision &&
      detailTargetKey() === targetKey &&
      accountStateVersion() === expectedAccountVersion
    );
  }

  function selectPage(next: number): void {
    if (!detail || next < 0 || next >= detail.pages.length) return;
    activePageIndex = next;
  }

  function openViewer(returnFocus: HTMLElement): void {
    if (detail?.pages[activePageIndex]?.current_media) {
      viewerReturnFocus = returnFocus;
      viewerPage = activePageIndex;
    }
  }

  async function moveToTrash(): Promise<void> {
    if (!detail) return;
    const current = detail.work;
    const retentionDays = trashRetentionDays;
    busy = 'trash';
    actionError = '';
    try {
      await moveWorkToTrash(current.id, retentionDays);
      if (!isCurrentWork(current.id)) return;
      await goto(resolve(detailReturnRoute(page.state)));
    } catch {
      if (isCurrentWork(current.id)) {
        actionError = '作品移入回收站失败';
      }
    } finally {
      if (isCurrentWork(current.id) && busy === 'trash') busy = '';
    }
  }

  async function restoreFromTrash(): Promise<void> {
    if (
      !detail ||
      detail.work.collection_state !== 'trash' ||
      !detail.trash_capabilities?.can_restore
    )
      return;
    const current = detail.work;
    busy = 'restore';
    notice = '';
    actionError = '';
    try {
      await restoreWork(current.id);
      if (!isCurrentWork(current.id)) return;
      const source = currentDetailSource(page.state);
      if (source?.kind === 'trash') removeWorkFromTrashReturn(current.id);
      await goto(resolve(source?.route ?? '/gallery'));
    } catch {
      if (isCurrentWork(current.id)) actionError = '作品移出回收站失败';
    } finally {
      if (isCurrentWork(current.id) && busy === 'restore') busy = '';
    }
  }

  function requestPurge(returnFocus: HTMLElement): void {
    if (!detail || detail.work.collection_state !== 'trash') return;
    actionError = '';
    purgeReturnFocus = returnFocus;
    purgeConfirmation = true;
  }

  async function purgeFromTrash(): Promise<void> {
    if (!detail || detail.work.collection_state !== 'trash') return;
    const current = detail.work;
    busy = 'purge';
    notice = '';
    actionError = '';
    try {
      await purgeWork(current.id);
      if (!isCurrentWork(current.id)) return;
      const visibleDetail = detail;
      if (visibleDetail) {
        detail = {
          ...visibleDetail,
          trash_capabilities: {
            can_restore: false,
            can_reschedule: false,
            blocked_reason: 'purge_queued'
          }
        };
      }
      purgeConfirmation = false;
      notice = '作品已加入后台清理队列';
    } catch {
      if (isCurrentWork(current.id)) actionError = '清理任务建立失败';
    } finally {
      if (isCurrentWork(current.id) && busy === 'purge') busy = '';
    }
  }
</script>

<svelte:head>
  <title>{detail?.work.title ?? '作品详情'} · PixivArchive</title>
</svelte:head>

{#if loadError}
  <div
    class:work-refresh-error={Boolean(detail)}
    class:work-load-error={!detail}
  >
    {#if loadRetryable}
      <RetryMessage
        message={loadError}
        actionLabel="重新读取作品详情"
        onRetry={() => detailRefresh.retry()}
      />
    {:else}
      <p class="inline-message error" role="alert">{loadError}</p>
    {/if}
  </div>
{/if}

{#if missingWork}
  {#snippet missingActions()}
    <a class="secondary-button" href={resolve('/gallery')}>返回图库</a>
  {/snippet}
  <div class="work-missing">
    <AlertBanner
      title="没有找到这个Pixiv作品"
      message="作品可能不存在，或已经完成物理清理。"
      actions={missingActions}
    />
  </div>
{/if}

{#if detail}
  <article class="work-detail">
    <WorkMediaPanel
      {detail}
      {activePageIndex}
      viewerOpen={viewerPage !== null}
      {ugoiraLimits}
      onSelectPage={selectPage}
      onOpenViewer={openViewer}
    />
    <WorkMetadataPanel
      {detail}
      {revisions}
      account={detail.work.collection_state === 'trash' ? null : actionAccount}
      bookmarkDisabled={detail.work.collection_state === 'trash' ||
        !actionAccount ||
        detailAccountId !== actionAccount.account_id}
      {busy}
      {notice}
      error={actionError}
      onToggleBookmark={() => void toggleBookmark()}
      onMoveToTrash={() => void moveToTrash()}
      onRestoreFromTrash={() => void restoreFromTrash()}
      {requestPurge}
    />
  </article>

  {#if viewerPage !== null}
    <UnifiedViewer
      {detail}
      initialPageIndex={viewerPage}
      {ugoiraLimits}
      returnFocus={viewerReturnFocus}
      onClose={() => (viewerPage = null)}
    />
  {/if}
{:else if !missingWork && !loadError}
  <EmptyState message="正在读取作品详情…" loading />
{/if}

{#if purgeConfirmation && detail}
  <ConfirmDialog
    title="立即清理作品"
    description={`立即清理“${detail.work.title}”的全部媒体和完整元数据？`}
    confirmLabel="立即清理"
    tone="danger"
    busy={busy === 'purge'}
    error={actionError}
    returnFocus={purgeReturnFocus}
    onConfirm={() => void purgeFromTrash()}
    onCancel={() => (purgeConfirmation = false)}
  />
{/if}

<style>
  .work-detail {
    display: grid;
    width: 100%;
    max-width: 100%;
    min-width: 0;
    grid-template-columns: minmax(0, 1.18fr) minmax(340px, 0.82fr);
    height: var(--main-viewport-height);
    gap: clamp(24px, 4vw, 58px);
    align-items: stretch;
    overflow: hidden;
  }

  .work-load-error {
    display: grid;
    padding: 2rem;
    place-content: center;
  }

  .work-missing {
    display: grid;
    min-height: min(360px, var(--main-viewport-height));
    place-content: center;
  }

  .work-refresh-error {
    margin-bottom: 1rem;
  }

  @media (max-width: 720px) {
    .work-detail {
      height: auto;
      grid-template-columns: 1fr;
      overflow: visible;
    }
  }
</style>
