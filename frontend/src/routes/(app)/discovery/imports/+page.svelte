<script lang="ts">
  import { onMount } from 'svelte';

  import {
    importApi,
    IMPORT_RUN_LIST_LIMIT,
    type ImportKind,
    type ImportRun,
    type ImportStrategy
  } from '$lib/api/imports';
  import { ruleWorkbenchApi, type RuleSummary } from '$lib/api/rules';
  import {
    AppEventRefreshCoordinator,
    currentAppEventVersion
  } from '$lib/app-event-refresh';
  import Button from '$lib/components/ui/Button.svelte';
  import DataTable from '$lib/components/ui/DataTable.svelte';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import Field from '$lib/components/ui/Field.svelte';
  import PageHeader from '$lib/components/ui/PageHeader.svelte';
  import PanelHeader from '$lib/components/ui/PanelHeader.svelte';
  import ReadableTime from '$lib/components/ui/ReadableTime.svelte';
  import RecentListCount from '$lib/components/ui/RecentListCount.svelte';
  import RetryMessage from '$lib/components/ui/RetryMessage.svelte';
  import SelectField from '$lib/components/ui/SelectField.svelte';
  import StatusPill from '$lib/components/ui/StatusPill.svelte';
  import TextField from '$lib/components/ui/TextField.svelte';
  import { parseSourceId } from '$lib/gallery-routes';
  import { importKindLabel, importStateLabel } from '$lib/labels';
  import { LatestRequest } from '$lib/latest-request';
  import {
    isPixivAccountConflict,
    pixivAccountActionFailureMessage
  } from '$lib/pixiv-account-errors';
  import { pixivAccountStore } from '$lib/stores/pixiv-account.svelte';

  const importResources = ['job', 'rule'] as const;
  let account = $derived(pixivAccountStore.currentForAction);
  let history = $state<ImportRun[]>([]);
  let rules = $state<RuleSummary[]>([]);
  let kind = $state<ImportKind>('artist');
  let pixivId = $state('');
  let strategyMode = $state<ImportStrategy['mode']>('default');
  let ruleId = $state('');
  let busy = $state(false);
  let loading = $state(true);
  let loadError = $state('');
  let actionError = $state('');
  const pageRequests = new LatestRequest();
  const pageRefresh = new AppEventRefreshCoordinator(loadPage);
  let publishedRules = $derived(
    rules.filter((rule) => rule.current_version !== null)
  );
  let accountReady = $derived(
    Boolean(account?.account_id && !pixivAccountStore.loading)
  );

  onMount(() => {
    pageRefresh.start(currentAppEventVersion(importResources));
    return () => {
      pageRefresh.dispose();
      pageRequests.invalidate();
    };
  });

  $effect(() => {
    pageRefresh.observe(currentAppEventVersion(importResources));
  });

  async function loadPage(): Promise<boolean> {
    const request = pageRequests.begin();
    loading = true;
    loadError = '';
    try {
      const [loadedHistory, loadedRules] = await Promise.all([
        importApi.list(),
        ruleWorkbenchApi.listRules()
      ]);
      if (!pageRequests.isCurrent(request)) return false;
      history = loadedHistory;
      rules = loadedRules;
      return true;
    } catch {
      if (pageRequests.isCurrent(request)) {
        loadError = '导入记录或规则暂时无法读取';
      }
      return false;
    } finally {
      if (pageRequests.isCurrent(request)) loading = false;
    }
  }

  async function queueImport(): Promise<void> {
    actionError = '';
    const targetId = parseSourceId(pixivId);
    const accountId = account?.account_id;
    if (!accountReady || !accountId || targetId === null) {
      actionError = '需要有效的Pixiv账户和ID';
      return;
    }
    if (
      strategyMode === 'rule' &&
      !publishedRules.some((rule) => rule.id === ruleId)
    ) {
      actionError = '请选择一条已发布的规则';
      return;
    }
    const strategy: ImportStrategy =
      strategyMode === 'rule'
        ? { mode: 'rule', rule_id: ruleId }
        : { mode: strategyMode };
    busy = true;
    try {
      const queued = await importApi.queue({
        account_id: accountId,
        kind,
        target_pixiv_id: targetId,
        strategy
      });
      if (pixivAccountStore.currentForAction?.account_id !== accountId) return;
      history = [queued, ...history.filter((run) => run.id !== queued.id)];
      pixivId = '';
    } catch (cause) {
      actionError = pixivAccountActionFailureMessage(cause, '导入任务建立失败');
      if (isPixivAccountConflict(cause)) void pixivAccountStore.load();
    } finally {
      busy = false;
    }
  }

  function strategyLabel(strategy: ImportRun['strategy']): string {
    switch (strategy.mode) {
      case 'default':
        return '默认下载';
      case 'rule':
        return '按规则采集';
      case 'forced':
        return '强制下载';
    }
  }

  function statusTone(
    status: string
  ): 'neutral' | 'success' | 'warning' | 'error' | 'primary' {
    if (status === 'failed' || status === 'blocked_by_deletion_marker') {
      return 'error';
    }
    if (status === 'queued' || status === 'running') return 'primary';
    if (status === 'ignored') return 'neutral';
    return 'success';
  }
</script>

<svelte:head>
  <title>手动导入 · PixivArchive</title>
</svelte:head>

<section class="workspace-page">
  <PageHeader title="手动导入" />

  {#if loadError}
    <RetryMessage
      message={loadError}
      busy={loading}
      actionLabel="重新读取导入记录和规则"
      onRetry={() => pageRefresh.retry()}
    />
  {/if}

  <div class="workspace-layout import-layout">
    <section class="panel">
      <PanelHeader title="建立一次性任务" />
      <div class="form-body">
        <div class="tab-list" role="tablist" aria-label="导入类型">
          <button
            type="button"
            role="tab"
            aria-selected={kind === 'artist'}
            onclick={() => (kind = 'artist')}>作者ID</button
          >
          <button
            type="button"
            role="tab"
            aria-selected={kind === 'work'}
            onclick={() => (kind = 'work')}>作品ID</button
          >
        </div>
        <TextField
          label="Pixiv ID"
          bind:value={pixivId}
          inputmode="numeric"
          autocomplete="off"
          placeholder={kind === 'artist' ? '输入作者ID' : '输入作品ID'}
        />
        <Field label="采集方式">
          <SelectField
            value={strategyMode}
            ariaLabel="采集方式"
            fullWidth
            options={[
              { value: 'default', label: '默认下载' },
              {
                value: 'rule',
                label: '按规则采集'
              },
              { value: 'forced', label: '强制下载' }
            ]}
            onChange={(value) =>
              (strategyMode = value as ImportStrategy['mode'])}
          />
        </Field>
        {#if strategyMode === 'rule'}
          <Field label="下载规则">
            <SelectField
              bind:value={ruleId}
              ariaLabel="下载规则"
              placeholder={publishedRules.length === 0
                ? '暂无已发布规则'
                : '选择已发布规则'}
              disabled={publishedRules.length === 0}
              fullWidth
              options={publishedRules.map((rule) => ({
                value: rule.id,
                label: `${rule.name} · v${rule.current_version}`
              }))}
            />
            {#if publishedRules.length === 0}
              <small>请先发布一条规则</small>
            {/if}
          </Field>
        {/if}
        <Button
          variant="primary"
          disabled={busy || !accountReady}
          onclick={queueImport}>建立导入任务</Button
        >
        {#if actionError}
          <p class="inline-message error" role="alert">{actionError}</p>
        {/if}
      </div>
    </section>

    <section class="panel">
      <PanelHeader title="最近导入">
        {#snippet actions()}
          <RecentListCount
            count={history.length}
            limit={IMPORT_RUN_LIST_LIMIT}
            loading={loading && history.length === 0}
          />
        {/snippet}
      </PanelHeader>
      <DataTable ariaLabel="最近导入记录" class="import-history">
        <thead>
          <tr>
            <th>目标</th>
            <th>采集方式</th>
            <th>状态</th>
            <th>发现 / 保存</th>
            <th>建立时间</th>
          </tr>
        </thead>
        <tbody>
          {#each history as run (run.id)}
            <tr>
              <td>
                <strong class="import-target">
                  {importKindLabel(run.kind)}
                  {run.target_pixiv_id}
                </strong>
              </td>
              <td>{strategyLabel(run.strategy)}</td>
              <td>
                <StatusPill
                  label={importStateLabel(run.status)}
                  tone={statusTone(run.status)}
                />
              </td>
              <td>{run.discovered_count} / {run.saved_count}</td>
              <td><ReadableTime value={run.created_at} /></td>
            </tr>
          {:else}
            <tr>
              <td colspan="5">
                <EmptyState message="还没有手动导入记录" />
              </td>
            </tr>
          {/each}
        </tbody>
      </DataTable>
    </section>
  </div>
</section>

<style>
  .import-layout {
    grid-template-columns: minmax(310px, 0.36fr) minmax(0, 1fr);
  }

  .import-target {
    color: var(--color-text-1);
    font-size: 0.82rem;
  }

  @media (max-width: 980px) {
    .import-layout {
      grid-template-columns: 1fr;
    }
  }
</style>
