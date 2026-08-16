<script lang="ts">
  import { browser } from '$app/environment';
  import {
    afterNavigate,
    beforeNavigate,
    disableScrollHandling
  } from '$app/navigation';
  import { onMount, tick } from 'svelte';
  import { SvelteSet } from 'svelte/reactivity';

  import {
    AppEventRefreshCoordinator,
    currentAppEventVersion
  } from '$lib/app-event-refresh';
  import {
    listTrash,
    purgeAllTrash,
    purgeWork,
    purgeWorks,
    rescheduleWorks,
    restoreWork,
    restoreWorks,
    type TrashFilter,
    type TrashSelectionExpression,
    type TrashPurgeState,
    type TrashCursor,
    type TrashSummary,
    type TrashWork
  } from '$lib/api/trash';
  import { ApiError } from '$lib/api/client';
  import ConfirmDialog from '$lib/components/ui/ConfirmDialog.svelte';
  import CountLabel from '$lib/components/ui/CountLabel.svelte';
  import Button from '$lib/components/ui/Button.svelte';
  import DateTimeField from '$lib/components/ui/DateTimeField.svelte';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import MetricStrip from '$lib/components/ui/MetricStrip.svelte';
  import PageHeader from '$lib/components/ui/PageHeader.svelte';
  import PanelHeader from '$lib/components/ui/PanelHeader.svelte';
  import RetryMessage from '$lib/components/ui/RetryMessage.svelte';
  import SelectionActions from '$lib/components/ui/SelectionActions.svelte';
  import { formatBytes } from '$lib/format';
  import { LatestRequest } from '$lib/latest-request';
  import {
    captureGalleryViewport,
    restoreGalleryViewport
  } from '$lib/stores/gallery-return';

  import TrashFilterBar from './TrashFilterBar.svelte';
  import {
    loadTrashSnapshot,
    mergeTrashSchedules,
    type TrashPageFilter
  } from './trash-pages';
  import { TrashSelectionSession } from './trash-selection-session.svelte';
  import { saveTrashReturn, takeTrashReturn } from './trash-return';
  import TrashWorkRow from './TrashWorkRow.svelte';

  type PurgeConfirmation =
    | { kind: 'single'; item: TrashWork }
    | { kind: 'batch'; expression: TrashSelectionExpression; count: number }
    | { kind: 'all'; count: number };

  const trashResources = ['work', 'job'] as const;
  const restored = browser ? takeTrashReturn() : null;
  let items = $state<TrashWork[]>(restored?.items ?? []);
  let summary = $state<TrashSummary>(restored?.summary ?? emptySummary());
  let allSummary = $state<TrashSummary>(restored?.allSummary ?? emptySummary());
  let nextCursor = $state<TrashCursor | null>(restored?.nextCursor ?? null);
  const selection = new TrashSelectionSession();
  let schedules = $state<Record<string, string>>(restored?.schedules ?? {});
  const dirtyScheduleIds = new SvelteSet(restored?.dirtyScheduleIds ?? []);
  let batchSchedule = $state('');
  let query = $state(restored?.query ?? '');
  let purgeStates = $state<TrashPurgeState[]>(restored?.purgeStates ?? []);
  let appliedQuery = $state(restored?.appliedQuery ?? '');
  let appliedPurgeStates = $state<TrashPurgeState[]>(
    restored?.appliedPurgeStates ?? []
  );
  let busy = $state(false);
  let loading = $state(false);
  let loadingMore = $state(false);
  let hasSnapshot = $state(Boolean(restored));
  let notice = $state('');
  let readError = $state('');
  let commandError = $state('');
  let purgeConfirmation = $state<PurgeConfirmation | null>(null);
  let purgeReturnFocus = $state<HTMLElement | null>(null);
  const latestLoad = new LatestRequest();
  const trashRefresh = new AppEventRefreshCoordinator(refresh);
  let trashMetrics = $derived([
    {
      label: '待处理作品',
      value: String(summary.total_count),
      valueSize: 'standard' as const
    },
    {
      label: '逻辑文件总量',
      value: formatBytes(summary.logical_bytes),
      valueSize: 'standard' as const
    },
    {
      label: '预计可回收空间',
      value: formatBytes(summary.estimated_reclaimable_bytes),
      valueSize: 'standard' as const
    }
  ]);
  let selectedCount = $derived(selection.count);
  let blockedCount = $derived(selection.blockedCount);
  let selectedIds = $derived(
    selection.idsFor(items.map((item) => item.work_id))
  );
  let appliedFilter = $derived<TrashFilter>({
    query: appliedQuery || null,
    purge_states: [...appliedPurgeStates]
  });

  beforeNavigate(rememberTrashPosition);

  afterNavigate(() => {
    if (restored) void restoreTrashPosition();
  });

  onMount(() => {
    trashRefresh.start(currentAppEventVersion(trashResources));
    if (restored) {
      disableScrollHandling();
      void refresh().then((refreshed) => {
        if (refreshed) {
          trashRefresh.markCurrent(currentAppEventVersion(trashResources));
        }
      });
    }
    return () => {
      trashRefresh.dispose();
      latestLoad.invalidate();
    };
  });

  function emptySummary(): TrashSummary {
    return {
      total_count: 0,
      logical_bytes: 0,
      estimated_reclaimable_bytes: 0
    };
  }

  function rememberTrashPosition(): void {
    if (!browser) return;
    saveTrashReturn({
      route: '/system/trash',
      query,
      purgeStates: [...purgeStates],
      appliedQuery,
      appliedPurgeStates: [...appliedPurgeStates],
      items: [...items],
      nextCursor,
      summary: { ...summary },
      allSummary: { ...allSummary },
      schedules: { ...schedules },
      dirtyScheduleIds: [...dirtyScheduleIds],
      viewport: captureGalleryViewport('.trash-work[data-trash-anchor]')
    });
  }

  async function restoreTrashPosition(): Promise<void> {
    if (!restored) return;
    await tick();
    await restoreGalleryViewport(
      restored.viewport,
      '.trash-work[data-trash-anchor]'
    );
  }

  $effect(() => {
    trashRefresh.observe(
      currentAppEventVersion(trashResources),
      selection.mode || busy || loading || loadingMore
    );
  });

  async function refresh(): Promise<boolean> {
    return replaceSnapshot(
      {
        query: appliedQuery,
        purgeStates: [...appliedPurgeStates]
      },
      Math.max(items.length, 1)
    );
  }

  async function replaceSnapshot(
    filter: TrashPageFilter,
    minimumItems: number
  ): Promise<boolean> {
    const token = latestLoad.begin();
    loading = true;
    loadingMore = false;
    readError = '';
    try {
      const trash = await loadTrashSnapshot(listTrash, filter, minimumItems);
      if (!latestLoad.isCurrent(token)) return false;
      items = trash.items;
      nextCursor = trash.next_cursor ?? null;
      summary = trash.summary;
      allSummary = trash.all_summary;
      schedules = mergeTrashSchedules(trash.items, schedules, dirtyScheduleIds);
      hasSnapshot = true;
      await selection.refreshVisible(items.map((item) => item.work_id));
      if (!latestLoad.isCurrent(token)) return false;
      return true;
    } catch {
      if (latestLoad.isCurrent(token)) {
        readError = '回收站数据暂时无法读取';
      }
      return false;
    } finally {
      if (latestLoad.isCurrent(token)) loading = false;
    }
  }

  async function applyFilters(): Promise<void> {
    const nextQuery = query.trim();
    const nextPurgeStates = [...purgeStates];
    const refreshed = await replaceSnapshot(
      { query: nextQuery, purgeStates: nextPurgeStates },
      1
    );
    if (refreshed) {
      appliedQuery = nextQuery;
      appliedPurgeStates = nextPurgeStates;
      trashRefresh.markCurrent(currentAppEventVersion(trashResources));
    }
  }

  async function loadMore(): Promise<void> {
    if (!nextCursor || loading || loadingMore) return;
    const token = latestLoad.begin();
    loadingMore = true;
    readError = '';
    try {
      const trash = await listTrash({
        query: appliedQuery,
        purgeStates: appliedPurgeStates,
        cursor: nextCursor
      });
      if (!latestLoad.isCurrent(token)) return;
      items = [...items, ...trash.items];
      nextCursor = trash.next_cursor ?? null;
      summary = trash.summary;
      allSummary = trash.all_summary;
      schedules = mergeTrashSchedules(trash.items, schedules, dirtyScheduleIds);
      await selection.refreshVisible(items.map((item) => item.work_id));
    } catch {
      if (latestLoad.isCurrent(token)) {
        readError = '更多回收站作品暂时无法读取';
      }
    } finally {
      if (latestLoad.isCurrent(token)) loadingMore = false;
    }
  }

  function toggleSelection(workId: string, checked: boolean): void {
    void selection.setWork(
      workId,
      checked,
      items.map((item) => item.work_id)
    );
  }

  function enterMultiSelect(): void {
    if (items.length === 0) return;
    selection.enter(appliedFilter);
  }

  function exitMultiSelect(): void {
    selection.exit();
    batchSchedule = '';
  }

  async function selectAll(): Promise<void> {
    await selection.selectAll(items.map((item) => item.work_id));
  }

  async function invertSelection(): Promise<void> {
    await selection.invert(items.map((item) => item.work_id));
  }

  async function restore(item: TrashWork): Promise<void> {
    await runCommand(
      () => restoreWork(item.work_id),
      '作品恢复失败',
      '作品已恢复',
      { applyLocal: () => removeLocalWorks([item.work_id]) }
    );
  }

  async function saveSchedule(item: TrashWork): Promise<void> {
    const scheduledPurgeAt = schedules[item.work_id];
    if (!scheduledPurgeAt) return;
    await runCommand(
      () =>
        rescheduleWorks(singleWorkExpression(item.work_id), scheduledPurgeAt),
      '清理日期更新失败',
      '清理日期已更新',
      {
        applyLocal: () => applyLocalSchedule([item.work_id], scheduledPurgeAt)
      }
    );
  }

  function requestPurge(item: TrashWork, returnFocus: HTMLElement): void {
    purgeReturnFocus = returnFocus;
    purgeConfirmation = { kind: 'single', item };
  }

  async function purge(item: TrashWork): Promise<void> {
    await runCommand(
      () => purgeWork(item.work_id),
      '清理任务建立失败',
      '作品已加入后台清理队列'
    );
  }

  async function batchRestore(): Promise<void> {
    if (selectedCount === 0 || blockedCount > 0) return;
    const expression = selection.snapshotExpression();
    await runCommand(
      () => restoreWorks(expression),
      '所选作品恢复失败',
      '所选作品已恢复',
      {
        exitSelection: true
      }
    );
  }

  async function batchReschedule(): Promise<void> {
    if (!batchSchedule) return;
    if (selectedCount === 0 || blockedCount > 0) return;
    const expression = selection.snapshotExpression();
    await runCommand(
      () => rescheduleWorks(expression, batchSchedule),
      '所选作品的清理日期更新失败',
      '所选作品的清理日期已更新',
      {
        exitSelection: true
      }
    );
  }

  function requestBatchPurge(returnFocus: HTMLElement): void {
    if (selectedCount > 0) {
      purgeReturnFocus = returnFocus;
      purgeConfirmation = {
        kind: 'batch',
        expression: selection.snapshotExpression(),
        count: selectedCount
      };
    }
  }

  function requestPurgeAll(returnFocus: HTMLElement): void {
    if (allSummary.total_count > 0) {
      purgeReturnFocus = returnFocus;
      purgeConfirmation = { kind: 'all', count: allSummary.total_count };
    }
  }

  async function confirmPurge(): Promise<void> {
    const confirmation = purgeConfirmation;
    if (!confirmation) return;
    if (confirmation.kind === 'single') {
      await purge(confirmation.item);
    } else if (confirmation.kind === 'batch') {
      await runCommand(
        () => purgeWorks(confirmation.expression),
        '所选作品的清理任务建立失败',
        '所选作品已加入后台清理队列',
        { exitSelection: true }
      );
    } else {
      await runCommand(
        () => purgeAllTrash(),
        '清空回收站任务建立失败',
        '整个回收站已加入后台清理队列',
        { exitSelection: true }
      );
    }
    purgeConfirmation = null;
  }

  async function runCommand(
    command: () => Promise<unknown>,
    failureMessage: string,
    successMessage: string,
    options: {
      applyLocal?: () => void;
      exitSelection?: boolean;
    } = {}
  ): Promise<void> {
    if (busy) return;
    busy = true;
    commandError = '';
    notice = '';
    try {
      await command();
      options.applyLocal?.();
      notice = successMessage;
      if (options.exitSelection) exitMultiSelect();
      const refreshed = await refresh();
      if (refreshed) {
        trashRefresh.markCurrent(currentAppEventVersion(trashResources));
      }
    } catch (cause) {
      commandError = trashCommandFailureMessage(cause, failureMessage);
    } finally {
      busy = false;
    }
  }

  function updateSchedule(workId: string, value: string): void {
    schedules[workId] = value;
    dirtyScheduleIds.add(workId);
  }

  function applyLocalSchedule(workIds: string[], value: string): void {
    const updated = new Set(workIds);
    for (const workId of updated) {
      schedules[workId] = value;
      dirtyScheduleIds.delete(workId);
    }
    items = items.map((item) =>
      updated.has(item.work_id) ? { ...item, scheduled_purge_at: value } : item
    );
  }

  function removeLocalWorks(workIds: string[]): void {
    const removed = new Set(workIds);
    items = items.filter((item) => !removed.has(item.work_id));
    schedules = Object.fromEntries(
      Object.entries(schedules).filter(([workId]) => !removed.has(workId))
    );
    for (const workId of removed) {
      dirtyScheduleIds.delete(workId);
    }
  }

  function singleWorkExpression(workId: string): TrashSelectionExpression {
    return {
      filter: { query: null, purge_states: [] },
      base_selected: false,
      exception_work_ids: [workId]
    };
  }

  function trashCommandFailureMessage(
    cause: unknown,
    fallback: string
  ): string {
    if (cause instanceof ApiError && cause.code === 'trash_selection_blocked') {
      const details = cause.details as Record<string, unknown>;
      const selected = details?.selected_count;
      const blocked = details?.blocked_count;
      if (typeof selected === 'number' && typeof blocked === 'number') {
        return `所选${selected}件作品中有${blocked}件已经进入清理流程，本次没有修改任何作品`;
      }
      return '所选作品中有作品已经进入清理流程，本次没有修改任何作品';
    }
    return cause instanceof ApiError ? cause.message : fallback;
  }
</script>

<svelte:head>
  <title>回收站 · PixivArchive</title>
</svelte:head>

<section class="workspace-page">
  <PageHeader title="回收站" />

  <MetricStrip
    class="trash-metrics"
    items={trashMetrics.map((metric) => ({ ...metric, loading: !hasSnapshot }))}
  />

  {#if notice}<p class="inline-message success">{notice}</p>{/if}
  {#if readError}
    <RetryMessage
      message={readError}
      busy={busy || loading}
      onRetry={() => void refresh()}
    />
  {/if}
  {#if commandError}
    <p class="inline-message error" role="alert">{commandError}</p>
  {/if}
  {#if selection.error}
    <p class="inline-message error" role="alert">{selection.error}</p>
  {/if}

  <section class="panel">
    <PanelHeader title="待清理作品" class="trash-panel-heading">
      {#snippet actions()}
        <div class="panel-actions">
          {#if !selection.mode}
            <TrashFilterBar
              bind:query
              purgeState={purgeStates[0] ?? ''}
              disabled={busy}
              onPurgeStateChange={(value) => {
                purgeStates = value ? [value] : [];
              }}
              onApply={() => void applyFilters()}
            />
          {/if}
          {#if selection.mode && selectedCount > 0}
            <span class="panel-count">
              {selectedCount}件已选择{blockedCount > 0
                ? `，${blockedCount}件已进入清理流程`
                : ''}
            </span>
          {/if}
          <CountLabel
            total={summary.total_count}
            loaded={items.length}
            unit="件"
            loading={!hasSnapshot}
            variant="panel"
          />
          {#if selection.mode}
            <SelectionActions
              label={`${selectedCount}件已选择`}
              showLabel={false}
              onSelectAll={() => void selectAll()}
              onInvert={() => void invertSelection()}
              onClear={() => selection.clear()}
              onExit={exitMultiSelect}
            >
              {#snippet actions()}
                <Button
                  disabled={busy || selectedCount === 0 || blockedCount > 0}
                  onclick={() => void batchRestore()}>批量恢复</Button
                >
                <div class="batch-schedule-field">
                  <span>统一清理时间</span>
                  <DateTimeField
                    value={batchSchedule}
                    ariaLabel="统一清理时间"
                    disabled={busy}
                    compact
                    onChange={(value) => (batchSchedule = value)}
                  />
                </div>
                <Button
                  disabled={busy ||
                    selectedCount === 0 ||
                    blockedCount > 0 ||
                    !batchSchedule}
                  onclick={() => void batchReschedule()}>批量修改日期</Button
                >
                <Button
                  variant="danger"
                  disabled={busy || selectedCount === 0}
                  onclick={(event) => requestBatchPurge(event.currentTarget)}
                  >批量立即清理</Button
                >
              {/snippet}
            </SelectionActions>
          {:else}
            <Button
              disabled={busy || !hasSnapshot || items.length === 0}
              onclick={enterMultiSelect}>多选</Button
            >
          {/if}
          <Button
            variant="danger"
            disabled={busy || allSummary.total_count === 0}
            onclick={(event) => requestPurgeAll(event.currentTarget)}
            >清空全部</Button
          >
        </div>
      {/snippet}
    </PanelHeader>

    <div class="trash-list">
      {#each items as item (item.work_id)}
        <TrashWorkRow
          {item}
          multiSelect={selection.mode}
          selected={selectedIds.has(item.work_id)}
          schedule={schedules[item.work_id]}
          {busy}
          onSelectionChange={(checked) =>
            toggleSelection(item.work_id, checked)}
          onScheduleChange={(value) => updateSchedule(item.work_id, value)}
          onRestore={() => void restore(item)}
          onSaveSchedule={() => void saveSchedule(item)}
          onPurge={(returnFocus) => requestPurge(item, returnFocus)}
          onOpenDetail={rememberTrashPosition}
        />
      {:else}
        {#if hasSnapshot}
          <EmptyState message="回收站是空的" />
        {:else if loading}
          <EmptyState message="正在读取回收站数据…" loading />
        {/if}
      {/each}
    </div>
    {#if nextCursor}
      <div class="load-more-row">
        <Button
          disabled={busy || loading || loadingMore}
          onclick={() => void loadMore()}
        >
          {loadingMore ? '正在读取' : '继续加载'}
        </Button>
      </div>
    {/if}
  </section>
</section>

{#if purgeConfirmation}
  <ConfirmDialog
    title={purgeConfirmation.kind === 'single'
      ? '立即清理作品'
      : purgeConfirmation.kind === 'batch'
        ? '批量立即清理'
        : '清空回收站'}
    description={purgeConfirmation.kind === 'single'
      ? `立即清理“${purgeConfirmation.item.title}”的全部媒体和完整元数据？`
      : purgeConfirmation.kind === 'batch'
        ? `立即清理所选的${purgeConfirmation.count}件作品？`
        : `立即为回收站中的${purgeConfirmation.count}件可清理作品建立后台任务？预计可回收${formatBytes(allSummary.estimated_reclaimable_bytes)}。`}
    confirmLabel="立即清理"
    tone="danger"
    {busy}
    returnFocus={purgeReturnFocus}
    onConfirm={() => void confirmPurge()}
    onCancel={() => (purgeConfirmation = null)}
  />
{/if}

<style>
  .panel-actions {
    display: flex;
    min-width: 0;
    flex-wrap: wrap;
    justify-content: flex-end;
    align-items: center;
    gap: 0.55rem;
  }

  .batch-schedule-field {
    display: flex;
    align-items: center;
    gap: 0.45rem;
    color: var(--color-text-3);
    font-size: 0.68rem;
  }

  .trash-list {
    display: grid;
  }

  .load-more-row {
    display: flex;
    justify-content: center;
    padding: 0.9rem;
  }

  @media (max-width: 680px) {
    :global(.panel-heading.trash-panel-heading) {
      align-items: stretch;
      flex-direction: column;
    }

    .panel-actions {
      align-items: stretch;
      flex-direction: column;
    }

    .panel-actions :global(.trash-filters) {
      width: 100%;
    }

    .batch-schedule-field {
      align-items: stretch;
      flex-direction: column;
    }
  }
</style>
