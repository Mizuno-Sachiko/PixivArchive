<script lang="ts">
  import { browser } from '$app/environment';
  import { beforeNavigate, disableScrollHandling } from '$app/navigation';
  import { onDestroy, onMount, tick, type Snippet } from 'svelte';

  import {
    type GalleryFilterGroup,
    type GallerySearch,
    type GalleryWork
  } from '$lib/api/gallery';
  import { systemApi } from '$lib/api/system';
  import Button from '$lib/components/ui/Button.svelte';
  import ConfirmDialog from '$lib/components/ui/ConfirmDialog.svelte';
  import CountLabel from '$lib/components/ui/CountLabel.svelte';
  import PixivSourceLink from '$lib/components/ui/PixivSourceLink.svelte';
  import SelectField from '$lib/components/ui/SelectField.svelte';
  import { appEventsStore } from '$lib/stores/app-events.svelte';
  import { openWorkDetail } from '$lib/stores/detail-navigation';
  import { createGalleryQueryStore } from '$lib/stores/gallery-query.svelte';
  import {
    captureGalleryViewport,
    restoreGalleryViewport,
    saveGalleryReturn,
    takeGalleryReturn
  } from '$lib/stores/gallery-return';

  import FilterDrawer from './FilterDrawer.svelte';
  import GalleryToolbar from './GalleryToolbar.svelte';
  import {
    GallerySearchSession,
    GallerySelectionSession
  } from './gallery-sessions.svelte';
  import {
    GalleryRefreshCoordinator,
    type GalleryResourceVersions
  } from './gallery-refresh';
  import GalleryWaterfall from './GalleryWaterfall.svelte';
  import PageHeader from '$lib/components/ui/PageHeader.svelte';
  import SelectionActions from '$lib/components/ui/SelectionActions.svelte';

  interface Props {
    title?: string;
    description?: string;
    externalUrl?: string;
    externalPlacement?: 'title' | 'description';
    baseGroups?: GalleryFilterGroup[];
    descriptionActions?: Snippet;
  }

  let {
    title = '图库',
    description = '',
    externalUrl,
    externalPlacement = 'title',
    baseGroups = [],
    descriptionActions
  }: Props = $props();
  const route = browser ? window.location.pathname : '';
  const restored = browser ? takeGalleryReturn(route) : null;
  const query = restored?.query ?? createGalleryQueryStore();
  const currentResourceVersions = (): GalleryResourceVersions => ({
    work: appEventsStore.resourceRevisions.work,
    pixivBookmark: appEventsStore.resourceRevisions.pixiv_bookmark,
    pixivAccount: appEventsStore.resourceRevisions.pixiv_account,
    snapshot: appEventsStore.snapshotRevision
  });
  const initialResourceVersions = restored
    ? {
        work: restored.workRevision ?? 0,
        pixivBookmark: restored.pixivBookmarkRevision ?? 0,
        pixivAccount: restored.pixivAccountRevision ?? 0,
        snapshot: restored.snapshotRevision ?? 0
      }
    : currentResourceVersions();
  const search = new GallerySearchSession({
    items: restored?.items,
    cursor: restored?.cursor,
    totalCount: restored?.totalCount,
    loadedDepth: restored?.loadedDepth,
    appliedQuery: restored?.appliedQuery ?? withBaseGroups(query.build())
  });
  const selection = new GallerySelectionSession();
  let loadedResourceVersions = $state<GalleryResourceVersions>({
    ...initialResourceVersions
  });
  const refreshCoordinator = new GalleryRefreshCoordinator(
    initialResourceVersions,
    async () => {
      const targetVersions = currentResourceVersions();
      if (!(await search.refreshFromAppliedQuery())) return false;
      await refreshSelectionProjection();
      loadedResourceVersions = targetVersions;
      return true;
    }
  );
  let preloadEnabled = $state(false);
  let workspaceReady = $state(false);
  let filtersOpen = $state(false);
  let trashConfirmCount = $state<number | null>(null);
  let trashReturnFocus = $state<HTMLElement | null>(null);
  let retentionDays = $state(30);
  let selectedIds = $derived(selection.idsFor(search.items));
  let selectedCount = $derived(selection.count);

  $effect(() => {
    refreshCoordinator.observe(
      currentResourceVersions(),
      !workspaceReady ||
        selection.mode ||
        selection.busy ||
        search.loading ||
        search.refreshing
    );
  });

  beforeNavigate(() => {
    rememberGalleryPosition();
  });

  onMount(() => {
    void loadRetentionDays();
    if (restored) disableScrollHandling();
    const ready = restored
      ? restored.items.length === 0
        ? runSearch(true, false)
        : Promise.resolve(true)
      : runSearch(true);
    void ready.finally(() => {
      preloadEnabled = true;
      if (restored) void restoreGallery();
      else workspaceReady = true;
    });
  });

  onDestroy(() => {
    refreshCoordinator.dispose();
    search.invalidate();
    selection.exit();
  });

  async function restoreGallery(): Promise<void> {
    if (!restored) return;
    await tick();
    await restoreGalleryViewport(
      restored.viewport ?? { scrollY: restored.scrollY },
      '.card-slot[data-gallery-anchor]'
    );
    const targetVersions = currentResourceVersions();
    if (await search.refreshLoadedItems()) {
      await refreshSelectionProjection();
      loadedResourceVersions = targetVersions;
      refreshCoordinator.markCurrent(targetVersions);
    }
    workspaceReady = true;
  }

  function rememberGalleryPosition(): void {
    if (!browser) return;
    const viewport = captureGalleryViewport('.card-slot[data-gallery-anchor]');
    const snapshot = search.snapshot();
    saveGalleryReturn({
      route,
      scrollY: viewport.scrollY,
      viewport,
      items: snapshot.items ?? [],
      cursor: snapshot.cursor,
      totalCount: snapshot.totalCount ?? 0,
      loadedDepth: snapshot.loadedDepth,
      workRevision: loadedResourceVersions.work,
      pixivBookmarkRevision: loadedResourceVersions.pixivBookmark,
      pixivAccountRevision: loadedResourceVersions.pixivAccount,
      snapshotRevision: loadedResourceVersions.snapshot,
      query,
      appliedQuery: snapshot.appliedQuery
    });
  }

  async function openDetail(
    target: Parameters<typeof openWorkDetail>[0]
  ): Promise<void> {
    rememberGalleryPosition();
    await openWorkDetail(target, {
      kind: 'gallery',
      route: route as `/${string}`
    });
  }

  async function loadRetentionDays(): Promise<void> {
    try {
      retentionDays = (await systemApi.settings()).storage.trash_retention_days;
    } catch {
      retentionDays = 30;
    }
  }

  function withBaseGroups(current: GallerySearch): GallerySearch {
    return {
      ...current,
      group_mode: 'all',
      groups: [...baseGroups, ...current.groups]
    };
  }

  async function runSearch(reset: boolean, useDraft = true): Promise<boolean> {
    if (selection.busy) return false;
    if (reset) {
      if (useDraft && query.validationError) {
        search.error = query.validationError;
        return false;
      }
      const targetVersions = currentResourceVersions();
      const request = useDraft
        ? withBaseGroups(query.build())
        : $state.snapshot(search.appliedQuery);
      const loaded = await search.reset(request);
      if (loaded) {
        loadedResourceVersions = targetVersions;
        refreshCoordinator.markCurrent(targetVersions);
        await refreshSelectionProjection();
      }
      return loaded;
    }
    const loaded = await search.loadNext();
    if (loaded) {
      await refreshSelectionProjection();
    }
    return loaded;
  }

  async function refreshSelectionProjection(): Promise<void> {
    await selection.refreshVisible(search.items.map((work) => work.id));
  }

  function toggleSelection(work: GalleryWork, checked: boolean): void {
    void selection.setWork(
      work.id,
      checked,
      search.items.map((item) => item.id)
    );
  }

  async function selectAll(): Promise<void> {
    await selection.selectAll(search.items.map((work) => work.id));
  }

  async function invertSelection(): Promise<void> {
    await selection.invert(search.items.map((work) => work.id));
  }

  async function trashSelected(): Promise<void> {
    const result = await selection.trash(retentionDays, search.loading);
    trashConfirmCount = null;
    if (!result) return;
    await runSearch(true, false);
  }

  function requestTrashSelected(returnFocus: HTMLElement): void {
    trashReturnFocus = returnFocus;
    trashConfirmCount = selectedCount;
  }
</script>

<svelte:head>
  <title>{title} · PixivArchive</title>
</svelte:head>

<section class="gallery-page">
  <PageHeader
    {title}
    variant="gallery"
    {description}
    showDescriptionTools={Boolean(
      descriptionActions || (externalUrl && externalPlacement === 'description')
    )}
  >
    {#snippet actions()}
      {#if externalUrl && externalPlacement === 'title'}
        <PixivSourceLink
          href={externalUrl}
          label={`在Pixiv打开${title}`}
          showText
        />
      {/if}
    {/snippet}
    {#snippet descriptionTools()}
      {#if descriptionActions}{@render descriptionActions()}{/if}
      {#if externalUrl && externalPlacement === 'description'}
        <PixivSourceLink href={externalUrl} label={`在Pixiv打开${title}`} />
      {/if}
    {/snippet}
  </PageHeader>

  <GalleryToolbar>
    {#if selection.mode}
      <SelectionActions
        label={`${selectedCount}件已选择`}
        onSelectAll={() => void selectAll()}
        onInvert={() => void invertSelection()}
        onClear={() => selection.clear()}
        onExit={() => selection.exit()}
      >
        {#snippet actions()}
          <Button
            variant="danger"
            disabled={selectedCount === 0 || selection.busy || search.loading}
            onclick={(event) => requestTrashSelected(event.currentTarget)}
            >移入回收站</Button
          >
        {/snippet}
      </SelectionActions>
    {:else}
      <form
        class="gallery-search-controls"
        onsubmit={(event) => {
          event.preventDefault();
          void runSearch(true);
        }}
      >
        <input
          type="search"
          bind:value={query.searchText}
          disabled={selection.busy}
          placeholder="搜索标题、作者、标签或Pixiv ID"
          aria-label="图库搜索"
        />
        <SelectField
          bind:value={query.query.sort_direction}
          disabled={selection.busy}
          ariaLabel="排序方向"
          options={[
            { value: 'ascending', label: '正序' },
            { value: 'descending', label: '倒序' }
          ]}
          onChange={() => void runSearch(true)}
        />
        <SelectField
          bind:value={query.query.sort_field}
          disabled={selection.busy}
          ariaLabel="排序字段"
          options={[
            { value: 'pixiv_id', label: 'Pixiv ID' },
            { value: 'local_updated_at', label: '本地更新时间' },
            { value: 'published_at', label: '发布时间' },
            { value: 'bookmark_count', label: '收藏数' },
            { value: 'title', label: '标题' }
          ]}
          onChange={() => void runSearch(true)}
        />
        <Button disabled={selection.busy} onclick={() => (filtersOpen = true)}
          >筛选条件</Button
        >
        <Button
          disabled={selection.busy}
          onclick={() => selection.enter(search.appliedQuery)}>多选</Button
        >
        <Button variant="primary" type="submit" disabled={selection.busy}
          >搜索</Button
        >
      </form>
    {/if}
  </GalleryToolbar>

  <div class="gallery-status" aria-live="polite">
    <CountLabel
      total={search.totalCount}
      loaded={search.items.length}
      unit="件作品"
      loading={search.loading}
      loadingText="正在读取作品…"
    />
    {#if selection.notice}<span class="inline-message success"
        >{selection.notice}</span
      >{/if}
    {#if search.error}
      <span class="inline-message error" role="alert">{search.error}</span>
      <Button
        disabled={search.loading || search.refreshing || selection.busy}
        onclick={() => void runSearch(true, false)}>重新加载</Button
      >
    {/if}
    {#if selection.error}<span class="inline-message error" role="alert"
        >{selection.error}</span
      >{/if}
  </div>

  <GalleryWaterfall
    items={search.items}
    loading={search.loading}
    hasMore={Boolean(search.cursor)}
    loadEnabled={preloadEnabled &&
      !search.paginationError &&
      !selection.busy &&
      !search.refreshing}
    selectionMode={selection.mode}
    {selectedIds}
    onLoadMore={() => void runSearch(false)}
    onSelect={toggleSelection}
    onOpen={(target) => void openDetail(target)}
  />
  {#if search.paginationError}
    <div class="pagination-error" role="alert">
      <span>{search.paginationError}</span>
      <Button
        disabled={search.loading || selection.busy}
        onclick={() => void runSearch(false)}>重新加载</Button
      >
    </div>
  {/if}
</section>

{#if filtersOpen}
  <FilterDrawer
    {query}
    onApply={() => void runSearch(true)}
    onClose={() => (filtersOpen = false)}
  />
{/if}

{#if trashConfirmCount !== null}
  <ConfirmDialog
    title="移入回收站"
    description={`将所选的${trashConfirmCount}件作品移入回收站？作品会保留${retentionDays}天。`}
    confirmLabel="移入回收站"
    tone="danger"
    busy={selection.busy}
    returnFocus={trashReturnFocus}
    onConfirm={() => void trashSelected()}
    onCancel={() => (trashConfirmCount = null)}
  />
{/if}

<style>
  .pagination-error {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 0.7rem;
    padding: 0.8rem;
    color: var(--color-text-2);
    font-size: 0.74rem;
  }
</style>
