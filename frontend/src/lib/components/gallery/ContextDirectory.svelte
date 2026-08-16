<script module lang="ts">
  export type DirectoryLoadFailure = 'reset' | 'next' | 'refresh' | null;

  export function retryResetsDirectory(
    failedLoad: DirectoryLoadFailure
  ): boolean {
    return failedLoad === 'reset';
  }
</script>

<script lang="ts">
  import { browser } from '$app/environment';
  import {
    afterNavigate,
    beforeNavigate,
    disableScrollHandling
  } from '$app/navigation';
  import type { Pathname } from '$app/types';
  import { onDestroy, onMount, tick } from 'svelte';

  import type { PixivAgeRating } from '$lib/api/gallery';
  import {
    captureGalleryViewport,
    loadGalleryContextData,
    loadGalleryContextReturn,
    restoreGalleryViewport,
    saveGalleryContextData,
    saveGalleryContextReturn
  } from '$lib/stores/gallery-return';
  import { appEventsStore } from '$lib/stores/app-events.svelte';
  import { systemApi } from '$lib/api/system';

  import CountLabel from '../ui/CountLabel.svelte';
  import ConfirmDialog from '../ui/ConfirmDialog.svelte';
  import Button from '$lib/components/ui/Button.svelte';
  import ContextCard from './ContextCard.svelte';
  import ContextCardSkeleton from './ContextCardSkeleton.svelte';
  import GalleryToolbar from './GalleryToolbar.svelte';
  import { GalleryContextSelectionSession } from './gallery-sessions.svelte';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import PageHeader from '$lib/components/ui/PageHeader.svelte';
  import SelectionActions from '$lib/components/ui/SelectionActions.svelte';
  import ViewportPager from './ViewportPager.svelte';
  import { loadDirectorySnapshot, type DirectoryPage } from './directory-pages';
  import {
    GalleryRefreshCoordinator,
    type GalleryResourceVersions
  } from './gallery-refresh';

  interface DirectoryItem {
    id: string;
    href: Pathname;
    anchor: string;
    title: string;
    eyebrow?: string;
    secondary?: string;
    workCount: number;
    coverUrl?: string | null;
    coverAgeRating?: PixivAgeRating | null;
  }

  interface DirectoryCache {
    query: string;
    items: DirectoryItem[];
    total: number;
    nextCursor: string | null;
    workRevision: number;
    pixivBookmarkRevision: number;
    pixivAccountRevision: number;
    snapshotRevision: number;
  }

  interface Props {
    title: string;
    kind: import('$lib/api/gallery').GalleryContextKind;
    unit: string;
    searchPlaceholder: string;
    loadingText: string;
    emptyText: string;
    emptySearchText: string;
    readErrorText: string;
    loadPage: (
      query: string,
      cursor: string | null,
      limit: number
    ) => Promise<DirectoryPage<DirectoryItem>>;
  }

  let {
    title,
    kind,
    unit,
    searchPlaceholder,
    loadingText,
    emptyText,
    emptySearchText,
    readErrorText,
    loadPage
  }: Props = $props();

  const route = browser ? window.location.pathname : '';
  const restored = browser ? loadGalleryContextReturn(route) : null;
  const restoredQuery = restored?.query ?? '';
  const cached = browser ? loadGalleryContextData<DirectoryCache>(route) : null;
  const canUseCache = cached?.query === restoredQuery;
  const pageSize = 48;
  const currentResourceVersions = (): GalleryResourceVersions => ({
    work: appEventsStore.resourceRevisions.work,
    pixivBookmark: appEventsStore.resourceRevisions.pixiv_bookmark,
    pixivAccount: appEventsStore.resourceRevisions.pixiv_account,
    snapshot: appEventsStore.snapshotRevision
  });
  const initialResourceVersions: GalleryResourceVersions = canUseCache
    ? {
        work: cached.workRevision ?? 0,
        pixivBookmark: cached.pixivBookmarkRevision ?? 0,
        pixivAccount: cached.pixivAccountRevision ?? 0,
        snapshot: cached.snapshotRevision ?? 0
      }
    : currentResourceVersions();

  let items = $state<DirectoryItem[]>(canUseCache ? [...cached.items] : []);
  let query = $state(restoredQuery);
  let appliedQuery = $state(restoredQuery);
  let total = $state(canUseCache ? cached.total : 0);
  let nextCursor = $state<string | null>(
    canUseCache ? cached.nextCursor : null
  );
  let hasLoadedPage = $state(canUseCache);
  let loading = $state(false);
  let refreshing = $state(false);
  let directoryReady = $state(false);
  let loadedResourceVersions = $state<GalleryResourceVersions>({
    ...initialResourceVersions
  });
  let loadRevision = 0;
  let preloadEnabled = $state(false);
  let error = $state('');
  let failedLoad = $state<DirectoryLoadFailure>(null);
  const selection = new GalleryContextSelectionSession(() => kind);
  let retentionDays = $state(30);
  let trashConfirmCount = $state<number | null>(null);
  let trashReturnFocus = $state<HTMLElement | null>(null);
  let selectedIds = $derived(selection.idsFor(items));
  let restorationStarted = false;
  let restorationCancelled = false;
  let resolveContentReady: () => void;
  const contentReady = new Promise<void>((resolveReady) => {
    resolveContentReady = resolveReady;
  });
  const refreshCoordinator = new GalleryRefreshCoordinator(
    initialResourceVersions,
    async () => {
      const targetVersions = currentResourceVersions();
      if (!(await refreshLoadedItems())) return false;
      loadedResourceVersions = targetVersions;
      return true;
    }
  );

  $effect(() => {
    refreshCoordinator.observe(
      currentResourceVersions(),
      !directoryReady ||
        loading ||
        refreshing ||
        selection.mode ||
        selection.busy
    );
  });

  beforeNavigate(rememberPosition);

  afterNavigate(() => {
    if (!restored || restorationStarted) return;
    restorationStarted = true;
    void contentReady.then(restorePosition);
  });

  onMount(() => {
    void loadRetentionDays();
    if (restored) disableScrollHandling();
    const ready = canUseCache ? Promise.resolve() : initialize();
    void ready.finally(() => {
      preloadEnabled = true;
      directoryReady = true;
      resolveContentReady();
    });
  });

  onDestroy(() => {
    refreshCoordinator.dispose();
    loadRevision += 1;
    selection.exit();
  });

  async function loadRetentionDays(): Promise<void> {
    try {
      retentionDays = (await systemApi.settings()).storage.trash_retention_days;
    } catch {
      retentionDays = 30;
    }
  }

  async function initialize(): Promise<void> {
    const targetVersions = currentResourceVersions();
    if (!(await load(true))) return;
    loadedResourceVersions = targetVersions;
    refreshCoordinator.markCurrent(targetVersions);
    const restoredCount = restored?.loaded?.items ?? pageSize;
    while (items.length < restoredCount && nextCursor !== null) {
      if (!(await load(false))) break;
    }
  }

  async function search(): Promise<void> {
    restorationCancelled = true;
    preloadEnabled = false;
    try {
      const targetVersions = currentResourceVersions();
      if (await load(true)) {
        loadedResourceVersions = targetVersions;
        refreshCoordinator.markCurrent(targetVersions);
      }
    } finally {
      preloadEnabled = true;
    }
  }

  async function load(reset: boolean): Promise<boolean> {
    if (
      (!reset && loading) ||
      (!reset && refreshing) ||
      (!reset && (!hasLoadedPage || nextCursor === null))
    ) {
      return false;
    }
    const cursor = reset ? null : nextCursor;
    const revision = reset ? ++loadRevision : loadRevision;
    const requestedQuery = reset ? query : appliedQuery;
    if (reset) refreshing = false;
    loading = true;
    error = '';
    try {
      const page = await loadPage(requestedQuery, cursor, pageSize);
      if (revision !== loadRevision) return false;
      items = reset ? page.items : [...items, ...page.items];
      total = page.total;
      nextCursor = page.nextCursor;
      hasLoadedPage = true;
      if (reset) appliedQuery = requestedQuery;
      failedLoad = null;
      await selection.refreshVisible(items.map((item) => item.id));
      return true;
    } catch {
      if (revision === loadRevision) {
        error = readErrorText;
        failedLoad = reset ? 'reset' : 'next';
      }
      return false;
    } finally {
      if (revision === loadRevision) {
        loading = false;
      }
    }
  }

  async function refreshLoadedItems(): Promise<boolean> {
    if (loading || refreshing) return false;
    const revision = loadRevision;
    const visibleDepth = Math.max(items.length, pageSize);
    error = '';
    refreshing = true;
    try {
      const snapshot = await loadDirectorySnapshot(
        loadPage,
        appliedQuery,
        pageSize,
        visibleDepth
      );
      if (revision !== loadRevision) return false;
      items = snapshot.items;
      total = snapshot.total;
      nextCursor = snapshot.nextCursor;
      hasLoadedPage = true;
      failedLoad = null;
      await selection.refreshVisible(items.map((item) => item.id));
      return true;
    } catch {
      if (revision === loadRevision) {
        error = readErrorText;
        failedLoad = 'refresh';
      }
      return false;
    } finally {
      if (revision === loadRevision) refreshing = false;
    }
  }

  async function retryDirectoryLoad(): Promise<void> {
    if (failedLoad === 'refresh') {
      error = '';
      refreshCoordinator.retry();
      return;
    }
    if (failedLoad === 'reset') {
      await search();
      return;
    }
    await load(false);
  }

  function rememberPosition(): void {
    if (!browser) return;
    saveGalleryContextData<DirectoryCache>(route, {
      query: appliedQuery,
      items: [...items],
      total,
      nextCursor,
      workRevision: loadedResourceVersions.work,
      pixivBookmarkRevision: loadedResourceVersions.pixivBookmark,
      pixivAccountRevision: loadedResourceVersions.pixivAccount,
      snapshotRevision: loadedResourceVersions.snapshot
    });
    saveGalleryContextReturn({
      route,
      query: appliedQuery,
      viewport: captureGalleryViewport('.context-card[data-gallery-anchor]'),
      loaded: { items: items.length }
    });
  }

  async function restorePosition(): Promise<void> {
    if (!restored || restorationCancelled) return;
    await tick();
    await restoreGalleryViewport(
      restored.viewport,
      '.context-card[data-gallery-anchor]'
    );
  }

  function toggleSelection(id: string, selected: boolean): void {
    void selection.setItem(
      id,
      selected,
      items.map((item) => item.id)
    );
  }

  async function selectAll(): Promise<void> {
    await selection.selectAll(items.map((item) => item.id));
  }

  async function invertSelection(): Promise<void> {
    await selection.invert(items.map((item) => item.id));
  }

  function requestTrashSelected(returnFocus: HTMLElement): void {
    trashReturnFocus = returnFocus;
    trashConfirmCount = selection.workCount;
  }

  async function trashSelected(): Promise<void> {
    const result = await selection.trash(retentionDays, loading || refreshing);
    trashConfirmCount = null;
    if (!result) return;
    await search();
  }
</script>

<svelte:head>
  <title>{title} · PixivArchive</title>
</svelte:head>

<section class="gallery-page">
  <PageHeader {title} variant="gallery" />
  <GalleryToolbar>
    {#if selection.mode}
      <SelectionActions
        label={`${selection.contextCount}个目录项已选择 · ${selection.workCount}件作品`}
        onSelectAll={() => void selectAll()}
        onInvert={() => void invertSelection()}
        onClear={() => selection.clear()}
        onExit={() => selection.exit()}
      >
        {#snippet actions()}
          <Button
            variant="danger"
            disabled={selection.workCount === 0 ||
              selection.busy ||
              loading ||
              refreshing}
            onclick={(event) => requestTrashSelected(event.currentTarget)}
            >移入回收站</Button
          >
        {/snippet}
      </SelectionActions>
    {:else}
      <form
        class="context-search-controls"
        onsubmit={(event) => {
          event.preventDefault();
          void search();
        }}
      >
        <input
          bind:value={query}
          type="search"
          placeholder={searchPlaceholder}
          disabled={selection.busy}
        />
        <Button
          disabled={selection.busy}
          onclick={() => selection.enter(appliedQuery)}>多选</Button
        >
        <Button variant="primary" type="submit" disabled={selection.busy}
          >搜索</Button
        >
      </form>
    {/if}
  </GalleryToolbar>
  <div class="gallery-status" aria-live="polite">
    <CountLabel {total} loaded={items.length} {unit} {loading} {loadingText} />
    {#if selection.notice}<span class="inline-message success"
        >{selection.notice}</span
      >{/if}
    {#if selection.error}<span class="inline-message error" role="alert"
        >{selection.error}</span
      >{/if}
  </div>

  {#if error && items.length === 0}
    <div class="context-read-error" role="alert">
      <span>{error}</span>
      <Button disabled={loading} onclick={() => void retryDirectoryLoad()}
        >重新加载</Button
      >
    </div>
  {:else}
    <div class="context-grid">
      {#each items as item (item.id)}
        <ContextCard
          {...item}
          selectionMode={selection.mode}
          selected={selectedIds.has(item.id)}
          onSelect={toggleSelection}
          onOpen={rememberPosition}
        />
      {:else}
        {#if loading}
          <ContextCardSkeleton count={12} />
        {:else}
          <EmptyState
            message={appliedQuery.trim() ? emptySearchText : emptyText}
          />
        {/if}
      {/each}
    </div>

    <ViewportPager
      enabled={preloadEnabled && !error}
      hasMore={hasLoadedPage && nextCursor !== null}
      {loading}
      onLoadMore={() => void load(false)}
    />
    {#if loading && items.length > 0}
      <div class="context-loading" aria-live="polite">{loadingText}</div>
    {/if}
    {#if error}
      <div class="context-pagination-error" role="alert">
        <span>{error}</span>
        <Button disabled={loading} onclick={() => void retryDirectoryLoad()}
          >重新加载</Button
        >
      </div>
    {/if}
  {/if}
</section>

{#if trashConfirmCount !== null}
  <ConfirmDialog
    title="移入回收站"
    description={`将所选目录中的${trashConfirmCount}件作品移入回收站？作品会保留${retentionDays}天。`}
    confirmLabel="移入回收站"
    tone="danger"
    busy={selection.busy}
    returnFocus={trashReturnFocus}
    onConfirm={() => void trashSelected()}
    onCancel={() => (trashConfirmCount = null)}
  />
{/if}

<style>
  .context-grid > :global(.empty-panel) {
    grid-column: 1 / -1;
  }

  .context-loading {
    color: var(--color-text-3);
    font-size: 0.72rem;
    text-align: center;
  }

  .context-read-error,
  .context-pagination-error {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 0.7rem;
    color: var(--color-text-2);
    font-size: 0.74rem;
  }
</style>
