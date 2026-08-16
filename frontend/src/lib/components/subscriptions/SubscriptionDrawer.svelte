<script lang="ts">
  import {
    subscriptionApi,
    type Subscription,
    type SubscriptionCursor,
    type SubscriptionRun
  } from '$lib/api/subscriptions';
  import type { RuleSummary } from '$lib/api/rules';
  import AlertBanner from '$lib/components/ui/AlertBanner.svelte';
  import Button from '$lib/components/ui/Button.svelte';
  import ConfirmDialog from '$lib/components/ui/ConfirmDialog.svelte';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import NumberField from '$lib/components/ui/NumberField.svelte';
  import PanelHeader from '$lib/components/ui/PanelHeader.svelte';
  import ReadableTime from '$lib/components/ui/ReadableTime.svelte';
  import StatusPill from '$lib/components/ui/StatusPill.svelte';
  import TextField from '$lib/components/ui/TextField.svelte';
  import { subscriptionTriggerLabel } from '$lib/labels';
  import {
    MAX_SUBSCRIPTION_INTERVAL_MINUTES,
    MAX_SUBSCRIPTION_LOOKBACK_PAGES,
    MIN_SUBSCRIPTION_INTERVAL_MINUTES,
    subscriptionScheduleError
  } from '$lib/subscription-schedule';
  import { subscriptionPresentation } from '$lib/subscription-status';

  import SubscriptionDefinition from './SubscriptionDefinition.svelte';

  interface Props {
    subscription: Subscription | null;
    rules: RuleSummary[];
    onSaved: (subscription: Subscription) => void;
    onDeleted: (id: string) => void;
  }

  let { subscription, rules, onSaved, onDeleted }: Props = $props();
  let activeId = $state<string | null>(null);
  let name = $state('');
  let intervalMinutes = $state(1440);
  let lookbackPages = $state(2);
  let cursors = $state<SubscriptionCursor[]>([]);
  let runs = $state<SubscriptionRun[]>([]);
  let showRuns = $state(false);
  let busy = $state(false);
  let message = $state('');
  let error = $state('');
  let deleteConfirmOpen = $state(false);
  let deleteError = $state('');
  let deleteReturnFocus = $state<HTMLElement | null>(null);
  let auxiliaryRevision = 0;
  let actionRevision = 0;
  let fixedSubscription = $derived(
    subscription?.kind === 'following' || subscription?.kind === 'bookmarks'
  );
  let status = $derived(
    subscription ? subscriptionPresentation(subscription) : null
  );

  $effect(() => {
    const current = subscription;
    if (!current) {
      activeId = null;
      cursors = [];
      runs = [];
      auxiliaryRevision += 1;
      actionRevision += 1;
      busy = false;
      return;
    }
    if (current.id === activeId) return;
    const request = ++auxiliaryRevision;
    actionRevision += 1;
    activeId = current.id;
    name = current.name;
    intervalMinutes = Number(current.schedule.interval_minutes ?? 1440);
    lookbackPages = Number(current.schedule.lookback_pages ?? 2);
    showRuns = false;
    message = '';
    error = '';
    deleteConfirmOpen = false;
    deleteError = '';
    deleteReturnFocus = null;
    busy = false;
    void loadAuxiliary(current.id, request);
  });

  async function loadAuxiliary(id: string, request: number): Promise<void> {
    try {
      const [loadedCursors, loadedRuns] = await Promise.all([
        subscriptionApi.cursors(id),
        subscriptionApi.runs(id)
      ]);
      if (request !== auxiliaryRevision || activeId !== id) return;
      cursors = loadedCursors;
      runs = loadedRuns;
    } catch {
      if (request === auxiliaryRevision && activeId === id) {
        error = '运行记录或游标暂时无法读取';
      }
    }
  }

  async function save(): Promise<void> {
    if (!subscription || busy) return;
    const scheduleError = subscriptionScheduleError(
      intervalMinutes,
      lookbackPages
    );
    if (scheduleError) {
      message = '';
      error = scheduleError;
      return;
    }
    const current = subscription;
    const request = ++actionRevision;
    busy = true;
    error = '';
    try {
      const updated = await subscriptionApi.update(current.id, {
        expected_revision: current.revision,
        enabled: current.enabled,
        account_id: current.account_id,
        rule_id: current.rule_id,
        name: name.trim(),
        interval_minutes: intervalMinutes,
        lookback_pages: lookbackPages,
        params: current.params,
        next_run_at: current.next_run_at
      });
      onSaved(updated);
      if (isCurrentAction(current.id, request)) {
        message = '订阅设置已经保存';
      }
    } catch {
      if (isCurrentAction(current.id, request)) {
        error = '订阅设置保存失败';
      }
    } finally {
      if (isCurrentAction(current.id, request)) busy = false;
    }
  }

  async function runNow(): Promise<void> {
    if (!subscription || busy || status?.blocksImmediateRun) return;
    const id = subscription.id;
    const request = ++actionRevision;
    busy = true;
    error = '';
    try {
      const accepted = await subscriptionApi.run(id);
      if (isCurrentAction(id, request)) {
        message =
          accepted.trigger_kind === 'merged_pending'
            ? '已合并一次待运行'
            : '已经加入定时采集队列';
        try {
          await refreshSubscription(id, request);
        } catch {
          if (isCurrentAction(id, request)) {
            error = '任务已经建立，但最新运行状态暂时无法读取';
          }
        }
      }
    } catch {
      if (isCurrentAction(id, request)) {
        error = '当前无法建立运行任务';
      }
    } finally {
      if (isCurrentAction(id, request)) busy = false;
    }
  }

  async function toggleEnabled(): Promise<void> {
    if (!subscription || busy) return;
    const current = subscription;
    const request = ++actionRevision;
    busy = true;
    message = '';
    error = '';
    try {
      const updated = await subscriptionApi.setEnabled(
        current.id,
        current.revision,
        !current.enabled
      );
      onSaved(updated);
      if (isCurrentAction(current.id, request)) {
        message = updated.enabled ? '订阅已经启用' : '订阅已经停用';
      }
    } catch {
      if (isCurrentAction(current.id, request)) {
        error = current.enabled ? '订阅停用失败' : '订阅启用失败';
      }
    } finally {
      if (isCurrentAction(current.id, request)) busy = false;
    }
  }

  async function stopCurrentRun(): Promise<void> {
    if (!subscription || busy) return;
    const id = subscription.id;
    const request = ++actionRevision;
    busy = true;
    message = '';
    error = '';
    try {
      const updated = await subscriptionApi.stop(id);
      onSaved(updated);
      if (isCurrentAction(id, request)) {
        message = '本次运行已经停止';
        try {
          await reloadAuxiliary(id, request);
        } catch {
          if (isCurrentAction(id, request)) {
            error = '运行已经停止，但运行记录暂时无法读取';
          }
        }
      }
    } catch {
      if (isCurrentAction(id, request)) {
        error = '当前没有可以停止的运行，或运行状态已经变化';
      }
    } finally {
      if (isCurrentAction(id, request)) busy = false;
    }
  }

  async function refreshSubscription(
    id: string,
    request: number
  ): Promise<void> {
    const updated = await subscriptionApi.get(id);
    if (!isCurrentAction(id, request)) return;
    onSaved(updated);
    await reloadAuxiliary(id, request);
  }

  async function reloadAuxiliary(id: string, request: number): Promise<void> {
    const [loadedCursors, loadedRuns] = await Promise.all([
      subscriptionApi.cursors(id),
      subscriptionApi.runs(id)
    ]);
    if (!isCurrentAction(id, request)) return;
    cursors = loadedCursors;
    runs = loadedRuns;
  }

  async function removeSubscription(): Promise<void> {
    if (!subscription || busy) return;
    const id = subscription.id;
    const revision = subscription.revision;
    const request = ++actionRevision;
    busy = true;
    deleteError = '';
    try {
      await subscriptionApi.remove(id, revision);
      if (isCurrentAction(id, request)) deleteConfirmOpen = false;
      onDeleted(id);
    } catch {
      if (isCurrentAction(id, request)) {
        deleteError = '订阅删除失败，请稍后重试';
      }
    } finally {
      if (isCurrentAction(id, request)) busy = false;
    }
  }

  function isCurrentAction(id: string, request: number): boolean {
    return activeId === id && actionRevision === request;
  }

  function openDeleteDialog(returnFocus: HTMLElement): void {
    deleteError = '';
    deleteReturnFocus = returnFocus;
    deleteConfirmOpen = true;
  }

  function cursorLabel(cursor: SubscriptionCursor): string {
    return cursor.cursor_kind === 'backfill' ? '历史补采游标' : '日常游标';
  }

  function cursorValue(cursor: SubscriptionCursor): string {
    if (typeof cursor.value.page === 'number') {
      return `page ${cursor.value.page}`;
    }
    return JSON.stringify(cursor.value);
  }
</script>

<aside class="panel subscription-drawer" role="region" aria-label="订阅详情">
  {#if subscription}
    <PanelHeader title={subscription.name}>
      {#snippet actions()}
        <StatusPill
          label={status?.label ?? '正在读取'}
          tone={status?.tone ?? 'neutral'}
        />
      {/snippet}
    </PanelHeader>

    <div class="form-body">
      {#if status?.accountMessage}
        <AlertBanner
          title={status.accountTitle ?? status.label}
          message={status.accountMessage}
          tone={status.tone === 'error' ? 'error' : 'warning'}
        />
      {/if}
      <SubscriptionDefinition {subscription} {rules} />

      {#if !fixedSubscription}
        <div class="field-grid">
          <TextField label="订阅名称" bind:value={name} disabled={busy} wide />
          <NumberField
            label="执行间隔（分钟）"
            min={MIN_SUBSCRIPTION_INTERVAL_MINUTES}
            max={MAX_SUBSCRIPTION_INTERVAL_MINUTES}
            step="1"
            bind:value={intervalMinutes}
            disabled={busy}
          />
          <NumberField
            label="补采最近多少期"
            min="0"
            max={MAX_SUBSCRIPTION_LOOKBACK_PAGES}
            step="1"
            bind:value={lookbackPages}
            disabled={busy}
          />
        </div>
      {/if}

      <div class="button-row">
        {#if !fixedSubscription}
          <Button variant="primary" {busy} onclick={save}>保存修改</Button>
        {/if}
        <Button {busy} onclick={toggleEnabled}>
          {subscription.enabled ? '停用订阅' : '启用订阅'}
        </Button>
        <Button
          variant="secondary"
          size="compact"
          disabled={busy || status?.blocksImmediateRun}
          onclick={runNow}>立即运行</Button
        >
        {#if subscription.pending_run || subscription.recent_state === 'running'}
          <Button variant="danger" {busy} onclick={stopCurrentRun}
            >停止本次运行</Button
          >
        {/if}
        {#if !fixedSubscription}
          <Button
            variant="danger"
            {busy}
            onclick={(event) => openDeleteDialog(event.currentTarget)}
            >删除订阅</Button
          >
        {/if}
      </div>

      {#if message}
        <p class="inline-message success">{message}</p>
      {/if}
      {#if error}
        <p class="inline-message error" role="alert">{error}</p>
      {/if}

      <section class="cursor-list" aria-label="采集游标">
        <h3>采集进度</h3>
        {#each cursors as cursor (`${cursor.cursor_kind}:${cursor.source_key}`)}
          <div>
            <span>{cursorLabel(cursor)}</span>
            <strong>{cursorValue(cursor)}</strong>
            <small>{cursor.source_key}</small>
          </div>
        {:else}
          <p class="inline-message">尚未保存来源游标</p>
        {/each}
      </section>

      <Button onclick={() => (showRuns = !showRuns)}>
        {showRuns ? '收起运行记录' : '查看运行记录'}
      </Button>

      {#if showRuns}
        <section class="run-list" aria-label="订阅运行记录">
          {#each runs as run (run.id)}
            <article>
              <div>
                <strong>{subscriptionTriggerLabel(run.trigger_kind)}</strong>
                <span><ReadableTime value={run.created_at} /></span>
              </div>
              <p>发现 {run.discovered_count} · 忽略 {run.ignored_count}</p>
            </article>
          {:else}
            <p class="inline-message">这条订阅还没有运行记录</p>
          {/each}
        </section>
      {/if}
    </div>
  {:else}
    <EmptyState message="未选择订阅" />
  {/if}
</aside>

{#if deleteConfirmOpen && subscription && !fixedSubscription}
  <ConfirmDialog
    title="删除订阅"
    description={`确定删除“${subscription.name}”？已经归档的作品不会受到影响。`}
    confirmLabel="删除订阅"
    tone="danger"
    {busy}
    error={deleteError}
    returnFocus={deleteReturnFocus}
    onConfirm={() => void removeSubscription()}
    onCancel={() => (deleteConfirmOpen = false)}
  />
{/if}

<style>
  .subscription-drawer {
    position: sticky;
    top: calc(var(--topbar-height) + var(--secondary-nav-height) + 18px);
    min-width: 0;
  }

  .cursor-list,
  .run-list {
    display: grid;
    gap: 0.55rem;
  }

  .cursor-list h3 {
    margin: 0;
    font-size: 0.82rem;
  }

  .cursor-list > div,
  .run-list article {
    display: grid;
    gap: 0.2rem;
    padding: 0.75rem;
    border-radius: var(--radius-sm);
    background: var(--color-surface-2);
  }

  .cursor-list > div {
    min-width: 0;
  }

  .cursor-list span,
  .cursor-list small,
  .run-list span,
  .run-list p {
    color: var(--color-text-3);
    font-size: 0.7rem;
  }

  .cursor-list strong {
    font: 0.8rem var(--font-mono);
  }

  .cursor-list strong,
  .cursor-list small {
    min-width: 0;
    overflow-wrap: anywhere;
  }

  .run-list article > div {
    display: flex;
    justify-content: space-between;
    gap: 0.8rem;
  }

  .run-list p {
    margin: 0;
  }
</style>
