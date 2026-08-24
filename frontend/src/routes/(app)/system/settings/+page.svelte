<script lang="ts">
  import { onMount } from 'svelte';

  import {
    systemApi,
    type EffectiveSettings,
    type SavedSetting,
    type SystemStatus
  } from '$lib/api/system';
  import {
    AppEventRefreshCoordinator,
    currentAppEventVersion
  } from '$lib/app-event-refresh';
  import CollectionSettingsCard from '$lib/components/settings/CollectionSettingsCard.svelte';
  import ContentSettingsCard from '$lib/components/settings/ContentSettingsCard.svelte';
  import PixivSettingsCard from '$lib/components/settings/PixivSettingsCard.svelte';
  import ProcessingSettingsCard from '$lib/components/settings/ProcessingSettingsCard.svelte';
  import QueueSettingsCard from '$lib/components/settings/QueueSettingsCard.svelte';
  import StorageSettingsCard from '$lib/components/settings/StorageSettingsCard.svelte';
  import SettingsCard from '$lib/components/settings/SettingsCard.svelte';
  import UgoiraSettingsCard from '$lib/components/settings/UgoiraSettingsCard.svelte';
  import Button from '$lib/components/ui/Button.svelte';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import PageHeader from '$lib/components/ui/PageHeader.svelte';
  import RetryMessage from '$lib/components/ui/RetryMessage.svelte';
  import StatusPill from '$lib/components/ui/StatusPill.svelte';
  import { formatBytes } from '$lib/format';
  import { LatestRequest } from '$lib/latest-request';
  import { withSettingRevision } from '$lib/settings-revisions';
  import { contentSettingsStore } from '$lib/stores/content-settings.svelte';
  import { overviewDecorationsStore } from '$lib/stores/overview-decorations.svelte';

  const settingsResources = ['system_setting', 'job'] as const;
  const maintenanceOperations = [
    {
      id: 'regenerate_derivatives',
      title: '重新生成浏览图',
      action: '重新生成浏览图'
    },
    {
      id: 'scan_expired_trash',
      title: '扫描到期回收站',
      action: '扫描到期作品'
    }
  ];

  let status = $state<SystemStatus | null>(null);
  let mediaUsageBytes = $state<number | null>(null);
  let settings = $state<EffectiveSettings | null>(null);
  let busyOperation = $state<string | null>(null);
  let message = $state('');
  let error = $state('');
  let loading = $state(false);
  const settingsRequests = new LatestRequest();
  const settingsRefresh = new AppEventRefreshCoordinator(loadSettings);
  let usedPercent = $derived.by(() => {
    const storage = status?.storage;
    if (!storage || storage.total_bytes <= 0) return 0;
    return Math.min(
      100,
      Math.max(
        0,
        ((storage.total_bytes - storage.available_bytes) /
          storage.total_bytes) *
          100
      )
    );
  });

  onMount(() => {
    settingsRefresh.start(currentAppEventVersion(settingsResources));
    return () => {
      settingsRefresh.dispose();
      settingsRequests.invalidate();
    };
  });

  $effect(() => {
    settingsRefresh.observe(currentAppEventVersion(settingsResources));
  });

  async function loadSettings(): Promise<boolean> {
    const request = settingsRequests.begin();
    loading = true;
    const loadedMediaUsage = systemApi.mediaUsage().catch(() => null);
    try {
      const [loadedStatus, loadedSettings] = await Promise.all([
        systemApi.status(),
        systemApi.settings()
      ]);
      if (!settingsRequests.isCurrent(request)) return false;
      status = loadedStatus;
      settings = loadedSettings;
      error = '';
      void loadedMediaUsage.then((usage) => {
        if (settingsRequests.isCurrent(request)) {
          mediaUsageBytes = usage?.media_directory_bytes ?? null;
        }
      });
      return true;
    } catch {
      if (settingsRequests.isCurrent(request)) {
        error = '系统设置暂时无法读取';
      }
      return false;
    } finally {
      if (settingsRequests.isCurrent(request)) loading = false;
    }
  }

  function applySavedSetting(saved: SavedSetting): void {
    if (status) status = withSettingRevision(status, saved);
  }

  function applyContentSettings(value: EffectiveSettings['content']): void {
    if (settings) settings = { ...settings, content: value };
    contentSettingsStore.replace(value);
  }

  async function queueMaintenance(operation: string): Promise<void> {
    busyOperation = operation;
    message = '';
    error = '';
    try {
      const accepted = await systemApi.maintenance(operation);
      message =
        accepted.queued_count === 0
          ? '当前没有符合条件的项目'
          : `维护任务已加入后台队列，共${accepted.queued_count}项`;
    } catch {
      error = '维护任务建立失败';
    } finally {
      busyOperation = null;
    }
  }
</script>

<svelte:head>
  <title>系统设置 · PixivArchive</title>
</svelte:head>

<section class="workspace-page">
  <PageHeader title="系统设置" />

  {#if message}
    <p class="settings-message">{message}</p>
  {/if}
  {#if error}
    <RetryMessage
      message={error}
      busy={loading}
      onRetry={() => settingsRefresh.retry()}
    />
  {/if}

  <div class="settings-grid">
    {#if settings}
      <QueueSettingsCard
        value={settings.queue}
        revision={status?.setting_revisions.queue}
        onsaved={applySavedSetting}
      />
      {#if settings.processing}
        <ProcessingSettingsCard
          value={settings.processing}
          revision={status?.setting_revisions.processing}
          onsaved={applySavedSetting}
        />
      {/if}
      <StorageSettingsCard
        value={settings.storage}
        activeMediaRoot={status?.storage.active_media_root ??
          settings.storage.media_root ??
          ''}
        revision={status?.setting_revisions.storage}
        onsaved={applySavedSetting}
      />
      <CollectionSettingsCard
        retry={settings.retry}
        derivative={settings.derivative}
        avifAvailable={status?.capabilities.avif_derivatives ?? false}
        revisions={{
          retry: status?.setting_revisions.retry,
          derivative: status?.setting_revisions.derivative
        }}
        onsaved={applySavedSetting}
      />
      <PixivSettingsCard
        value={settings.pixiv}
        revision={status?.setting_revisions.pixiv}
        onsaved={applySavedSetting}
      />
      <ContentSettingsCard
        value={settings.content}
        revision={status?.setting_revisions.content}
        onsaved={applySavedSetting}
        onvaluechange={applyContentSettings}
        onshuffle={() => overviewDecorationsStore.shuffle()}
      />
      {#if settings.ugoira}
        <UgoiraSettingsCard
          value={settings.ugoira}
          revision={status?.setting_revisions.ugoira}
          onsaved={applySavedSetting}
        />
      {/if}
    {/if}

    <SettingsCard title="存储状态" class="capacity-panel">
      {#snippet headerActions()}
        {#if status?.storage}
          <StatusPill
            label={status.storage.write_stopped
              ? '媒体写入已停止'
              : '媒体写入正常'}
            tone={status.storage.write_stopped ? 'error' : 'success'}
          />
        {/if}
      {/snippet}
      {#if status?.storage}
        <div class="capacity-values">
          <div>
            <span>可用空间</span>
            <strong>{formatBytes(status.storage.available_bytes)}</strong>
          </div>
          <div>
            <span>总容量</span>
            <strong>{formatBytes(status.storage.total_bytes)}</strong>
          </div>
          <div>
            <span>媒体目录大小</span>
            <strong
              >{mediaUsageBytes === null
                ? '—'
                : formatBytes(mediaUsageBytes)}</strong
            >
          </div>
          <div>
            <span>预警阈值</span>
            <strong
              >{formatBytes(status.storage.warning_threshold_bytes)}</strong
            >
          </div>
          <div>
            <span>停止写入阈值</span>
            <strong
              >{formatBytes(status.storage.write_stop_threshold_bytes)}</strong
            >
          </div>
        </div>
        <div
          class="capacity-track"
          aria-label={`存储已使用${usedPercent.toFixed(1)}%`}
        >
          <span style={`width: ${usedPercent}%`}></span>
        </div>
        <div class="capability-row">
          <span
            >WebP {status.capabilities?.webp_derivatives
              ? '可用'
              : '不可用'}</span
          >
          <span
            >AVIF {status.capabilities?.avif_derivatives
              ? '可用'
              : '不可用'}</span
          >
        </div>
      {:else}
        <EmptyState message="存储状态暂时无法读取" />
      {/if}
    </SettingsCard>

    <SettingsCard title="后台维护">
      <div class="maintenance-list">
        {#each maintenanceOperations as operation (operation.id)}
          <div>
            <strong>{operation.title}</strong>
            <Button
              disabled={busyOperation !== null}
              onclick={() => void queueMaintenance(operation.id)}
            >
              {busyOperation === operation.id
                ? '正在加入队列'
                : operation.action}
            </Button>
          </div>
        {/each}
      </div>
    </SettingsCard>
  </div>
</section>

<style>
  .settings-message {
    padding: 0.75rem 0.9rem;
    margin: 0;
    border-radius: var(--radius-sm);
    background: var(--color-primary-soft);
    color: var(--color-primary);
    font-size: 0.76rem;
    font-weight: 700;
  }

  .settings-grid {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    gap: 1rem;
    align-items: start;
  }

  .capacity-values {
    display: grid;
    grid-template-columns: repeat(5, minmax(0, 1fr));
    gap: 0.8rem;
  }

  .capacity-values > div {
    display: grid;
    gap: 0.3rem;
    padding: 0.85rem;
    border-radius: var(--radius-sm);
    background: var(--color-surface-2);
  }

  .capacity-values span,
  .capability-row {
    color: var(--color-text-3);
    font-size: 0.7rem;
  }

  .capacity-track {
    height: 10px;
    overflow: hidden;
    border-radius: var(--radius-pill);
    background: var(--color-surface-3);
  }

  .capacity-track span {
    display: block;
    height: 100%;
    border-radius: inherit;
    background: var(--color-primary);
  }

  .capability-row {
    display: flex;
    flex-wrap: wrap;
    gap: 1rem;
  }

  .maintenance-list {
    display: grid;
    gap: 0.75rem;
  }

  .maintenance-list > div {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(148px, 160px);
    align-items: center;
    gap: 1rem;
  }

  .maintenance-list strong {
    font-size: 0.8rem;
    line-height: 1.4;
  }

  .maintenance-list :global(button) {
    width: 100%;
  }

  @media (max-width: 900px) {
    .capacity-values {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }

  @media (max-width: 560px) {
    .maintenance-list > div {
      grid-template-columns: 1fr;
      gap: 0.55rem;
      padding: 0.8rem 0;
    }
  }
</style>
