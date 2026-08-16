<script lang="ts">
  import { resolve } from '$app/paths';
  import { onMount } from 'svelte';

  import { ApiError, apiRequest } from '$lib/api/client';
  import { endpoints } from '$lib/api/endpoints';
  import type { SystemStatus } from '$lib/api/system';
  import {
    AppEventRefreshCoordinator,
    currentAppEventVersion
  } from '$lib/app-event-refresh';
  import OverviewQuickLink from '$lib/components/overview/OverviewQuickLink.svelte';
  import AlertBanner from '$lib/components/ui/AlertBanner.svelte';
  import Button from '$lib/components/ui/Button.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import MetricStrip from '$lib/components/ui/MetricStrip.svelte';
  import PageHeader from '$lib/components/ui/PageHeader.svelte';
  import RetryMessage from '$lib/components/ui/RetryMessage.svelte';
  import { formatCount, formatExactCount } from '$lib/format';
  import { LatestRequest } from '$lib/latest-request';
  import { TASK_PRIORITIES, taskPriorityLabel } from '$lib/labels';
  import { decorationSlots } from '$lib/overview-decorations';
  import { pixivAccountNotice } from '$lib/pixiv-account-status';
  import { pixivAccountStore } from '$lib/stores/pixiv-account.svelte';
  import { contentSettingsStore } from '$lib/stores/content-settings.svelte';
  import { overviewDecorationsStore } from '$lib/stores/overview-decorations.svelte';

  const statusResources = ['system_setting', 'job'] as const;
  let status = $state<SystemStatus | null>(null);
  let loading = $state(true);
  let loadError = $state('');
  const statusRequests = new LatestRequest();
  const statusRefresh = new AppEventRefreshCoordinator(loadStatus);

  let queueTotals = $derived.by(() => {
    let queued = 0;
    let running = 0;
    for (const queue of Object.values(status?.queue ?? {})) {
      queued += waitingCount(queue);
      running += queue.running ?? 0;
    }
    return { queued, running };
  });
  let overviewMetrics = $derived([
    {
      label: '队列等待',
      value: status ? formatCount(queueTotals.queued) : '—',
      title: status ? formatExactCount(queueTotals.queued) : undefined
    },
    {
      label: '正在处理',
      value: status ? formatCount(queueTotals.running) : '—',
      title: status ? formatExactCount(queueTotals.running) : undefined
    },
    {
      label: '数据库',
      value: status
        ? {
            kind: 'status' as const,
            label: statusLabel(status.database.status),
            tone: statusTone(status.database.status)
          }
        : {
            kind: 'status' as const,
            label: '读取中',
            tone: 'neutral' as const
          }
    },
    {
      label: '媒体目录',
      value: status
        ? {
            kind: 'status' as const,
            label: mediaStatusLabel(status.media.status),
            tone: statusTone(status.media.status)
          }
        : {
            kind: 'status' as const,
            label: '读取中',
            tone: 'neutral' as const
          }
    }
  ]);
  let accountAlert = $derived.by(() => {
    const account = pixivAccountStore.current;
    if (!account) {
      return pixivAccountStore.error
        ? {
            title: 'Pixiv账户状态读取失败',
            message: pixivAccountStore.error,
            tone: 'error' as const
          }
        : null;
    }
    return pixivAccountNotice(account.state);
  });
  let quickLinkDecorations = $derived(
    decorationSlots(overviewDecorationsStore.current)
  );
  let maskQuickLinkDecorations = $derived(
    !contentSettingsStore.effective.overview_allow_nsfw ||
      contentSettingsStore.effective.mask_non_all_age_thumbnails
  );

  function waitingCount(queue: Record<string, number>): number {
    return (
      (queue.queued ?? 0) +
      (queue.waiting_account ?? 0) +
      (queue.waiting_storage ?? 0)
    );
  }

  $effect(() => {
    statusRefresh.observe(currentAppEventVersion(statusResources));
  });

  onMount(() => {
    statusRefresh.start(currentAppEventVersion(statusResources));
    void overviewDecorationsStore.load();
    return () => {
      statusRefresh.dispose();
      statusRequests.invalidate();
    };
  });

  async function loadStatus(): Promise<boolean> {
    const request = statusRequests.begin();
    loading = true;
    try {
      const loaded = await apiRequest<SystemStatus>(endpoints.systemStatus);
      if (!statusRequests.isCurrent(request)) return false;
      status = loaded;
      loadError = '';
      return true;
    } catch (error) {
      if (statusRequests.isCurrent(request)) {
        loadError =
          error instanceof ApiError &&
          error.code === 'media_storage_unavailable'
            ? '无法读取媒体存储空间'
            : '请稍后重试';
      }
      return false;
    } finally {
      if (statusRequests.isCurrent(request)) loading = false;
    }
  }

  function statusLabel(value: string): string {
    if (value === 'healthy' || value === 'ready') return '正常';
    if (value === 'warning') return '需留意';
    return '异常';
  }

  function statusTone(value: string): 'success' | 'warning' | 'error' {
    if (value === 'healthy' || value === 'ready') return 'success';
    if (value === 'warning') return 'warning';
    return 'error';
  }

  function mediaStatusLabel(value: string): string {
    if (value === 'healthy' || value === 'ready') return '正常';
    if (value === 'warning') return '空间不足';
    return '目录不可用';
  }

  function serviceComponents(current: SystemStatus) {
    return [
      { name: '工作进程', ...current.worker },
      { name: 'PostgreSQL', ...current.database },
      {
        name: '媒体目录',
        status: current.media.status,
        message: mediaStatusLabel(current.media.status)
      }
    ];
  }
</script>

<svelte:head>
  <title>概览 · PixivArchive</title>
</svelte:head>

<section class="overview">
  <PageHeader title="概览" variant="page" />

  {#if loadError}
    <div class="read-error">
      <AlertBanner title="系统状态读取失败" message={loadError} tone="error" />
      <Button disabled={loading} onclick={() => statusRefresh.retry()}
        >重新读取</Button
      >
    </div>
  {:else if accountAlert}
    <AlertBanner
      title={accountAlert.title}
      message={accountAlert.message}
      tone={accountAlert.tone}
    />
  {:else if status?.storage?.write_stopped}
    <AlertBanner
      title="媒体写入已暂停"
      message="可用空间不足，已暂停媒体写入"
      tone="error"
    />
  {:else if status?.storage && status.storage.available_bytes <= status.storage.warning_threshold_bytes}
    <AlertBanner title="存储提醒" message="媒体存储空间不足" />
  {:else if status?.media.status === 'warning'}
    <AlertBanner title="存储提醒" message="媒体存储空间不足" />
  {:else if status?.media.message}
    <AlertBanner title="存储提醒" message={status.media.message} />
  {/if}

  <MetricStrip
    class="overview-metrics"
    appearance="overview"
    items={overviewMetrics}
  />

  <div class="overview-grid">
    <section class="queue-board solid-surface">
      <header>
        <div>
          <span class="section-icon"><Icon name="queue" size={20} /></span>
          <div>
            <h2>任务队列</h2>
          </div>
        </div>
        <a href={resolve('/tasks')}>查看全部任务</a>
      </header>

      <div class="queue-list">
        {#if status}
          {#each TASK_PRIORITIES as priority (priority)}
            {@const values = status.queue[priority] ?? {}}
            <div class="queue-row">
              <span class="queue-name">{taskPriorityLabel(priority)}</span>
              <div class="queue-meter">
                <i
                  style:width={`${Math.min(100, (waitingCount(values) + (values.running ?? 0)) * 5)}%`}
                ></i>
              </div>
              <span><b>{values.running ?? 0}</b>运行</span>
              <span><b>{waitingCount(values)}</b>等待</span>
            </div>
          {/each}
        {:else}
          <p class="empty-state">队列状态正在读取</p>
        {/if}
      </div>
    </section>

    <section class="service-board solid-surface">
      <header>
        <span class="section-icon"><Icon name="database" size={20} /></span>
        <div>
          <h2>运行组件</h2>
        </div>
      </header>

      <div class="service-list">
        {#if status}
          {#each serviceComponents(status) as component (component.name)}
            <div>
              <span>
                <i class={statusTone(component.status)}></i>
                {component.name}
              </span>
              <small>{component.message ?? statusLabel(component.status)}</small
              >
            </div>
          {/each}
        {:else}
          <p class="empty-state">正在读取组件状态</p>
        {/if}
      </div>
    </section>

    <section class="quick-board">
      {#if overviewDecorationsStore.error}
        <RetryMessage
          message={overviewDecorationsStore.error}
          busy={overviewDecorationsStore.loading}
          actionLabel="重新读取概览装饰图"
          onRetry={() => void overviewDecorationsStore.load()}
        />
      {/if}
      <div class="quick-links">
        <OverviewQuickLink
          href="/gallery"
          label="打开图库"
          decoration={quickLinkDecorations[0].decoration}
          maskNonAllAge={maskQuickLinkDecorations}
        />
        <OverviewQuickLink
          href="/discovery/subscriptions"
          label="查看订阅"
          decoration={quickLinkDecorations[1].decoration}
          maskNonAllAge={maskQuickLinkDecorations}
        />
        <OverviewQuickLink
          href="/rules"
          label="编辑规则"
          decoration={quickLinkDecorations[2].decoration}
          maskNonAllAge={maskQuickLinkDecorations}
        />
      </div>
    </section>
  </div>
</section>

<style>
  .overview {
    display: grid;
    gap: 22px;
  }

  .read-error {
    display: grid;
    justify-items: start;
    gap: 0.65rem;
  }

  .overview-grid {
    display: grid;
    grid-template-columns: minmax(0, 1.65fr) minmax(280px, 0.75fr);
    gap: 22px;
  }

  .queue-board,
  .service-board {
    border-radius: var(--radius-md);
  }

  .queue-board > header,
  .service-board > header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 1.2rem 1.3rem;
    border-bottom: 1px solid var(--color-border);
  }

  .queue-board > header > div,
  .service-board > header {
    display: flex;
    gap: 0.85rem;
    align-items: center;
  }

  .section-icon {
    display: grid;
    width: 38px;
    height: 38px;
    place-items: center;
    border-radius: 10px;
    background: var(--color-primary-soft);
    color: var(--color-primary);
  }

  h2 {
    margin: 0;
    font-size: 1rem;
    letter-spacing: -0.015em;
  }

  .queue-board header a {
    color: var(--color-primary);
    font-size: 0.78rem;
    font-weight: 650;
  }

  .queue-list {
    padding: 0.45rem 1.3rem 0.7rem;
  }

  .queue-row {
    display: grid;
    grid-template-columns: 110px minmax(80px, 1fr) 64px 64px;
    gap: 1rem;
    align-items: center;
    min-height: 54px;
    border-bottom: 1px solid var(--color-border);
    color: var(--color-text-3);
    font-size: 0.72rem;
  }

  .queue-row:last-child {
    border-bottom: 0;
  }

  .queue-name {
    color: var(--color-text-1);
    font-size: 0.82rem;
    font-weight: 650;
  }

  .queue-meter {
    height: 5px;
    overflow: hidden;
    border-radius: var(--radius-pill);
    background: var(--color-surface-3);
  }

  .queue-meter i {
    display: block;
    min-width: 4px;
    height: 100%;
    border-radius: inherit;
    background: var(--color-primary);
  }

  .queue-row b {
    margin-right: 0.2rem;
    color: var(--color-text-1);
    font-size: 0.82rem;
  }

  .service-board > header {
    justify-content: flex-start;
  }

  .service-list {
    padding: 0.55rem 1.25rem;
  }

  .service-list > div {
    display: flex;
    min-height: 47px;
    align-items: center;
    justify-content: space-between;
    border-bottom: 1px solid var(--color-border);
  }

  .service-list > div:last-child {
    border-bottom: 0;
  }

  .service-list span {
    display: flex;
    align-items: center;
    gap: 0.55rem;
    font-size: 0.8rem;
    font-weight: 620;
  }

  .service-list i {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--color-text-3);
  }

  .service-list i.success {
    background: var(--color-success);
  }

  .service-list i.warning {
    background: var(--color-warning);
  }

  .service-list i.error {
    background: var(--color-error);
  }

  .service-list small {
    max-width: 150px;
    overflow: hidden;
    color: var(--color-text-3);
    font-size: 0.7rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .quick-board {
    display: grid;
    grid-column: 1 / -1;
    gap: 0.75rem;
    padding: 0.75rem 0 0;
    border-top: 0;
    border-bottom: 0;
  }

  .quick-links {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 0.75rem;
  }

  .empty-state {
    padding: 1.25rem 0;
    color: var(--color-text-3);
    font-size: 0.8rem;
  }

  @media (max-width: 900px) {
    .overview-grid {
      grid-template-columns: 1fr;
    }

    .quick-board {
      grid-column: auto;
    }
  }

  @media (max-width: 620px) {
    .queue-row {
      grid-template-columns: 95px 1fr;
      gap: 0.55rem 0.8rem;
      padding: 0.55rem 0;
    }

    .queue-row > span:nth-last-child(-n + 2) {
      grid-column: auto;
    }

    .quick-links {
      grid-template-columns: 1fr;
    }
  }
</style>
