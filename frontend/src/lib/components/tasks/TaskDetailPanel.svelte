<script lang="ts">
  import type { TaskDetail } from '$lib/api/tasks';
  import Button from '$lib/components/ui/Button.svelte';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import KeyValueList from '$lib/components/ui/KeyValueList.svelte';
  import PanelHeader from '$lib/components/ui/PanelHeader.svelte';
  import ReadableTime from '$lib/components/ui/ReadableTime.svelte';
  import StatusPill from '$lib/components/ui/StatusPill.svelte';
  import { formatElapsed } from '$lib/format';
  import {
    errorClassLabel,
    taskKindLabel,
    taskPriorityLabel,
    taskStateLabel
  } from '$lib/labels';
  import { taskStateTone } from '$lib/task-status';

  interface Props {
    detail: TaskDetail | null;
    onRetry: () => Promise<void>;
    onCancel: () => Promise<void>;
  }

  let { detail, onRetry, onCancel }: Props = $props();
  let busy = $state(false);
  let message = $state('');
  let error = $state('');
  let activeTaskId = $state<string | null>(null);
  let actionRevision = 0;

  $effect(() => {
    const taskId = detail?.task.id ?? null;
    if (taskId === activeTaskId) return;
    activeTaskId = taskId;
    actionRevision += 1;
    busy = false;
    message = '';
    error = '';
  });

  async function runAction(action: 'retry' | 'cancel'): Promise<void> {
    const taskId = detail?.task.id;
    if (!taskId || busy) return;
    const request = ++actionRevision;
    busy = true;
    error = '';
    try {
      if (action === 'retry') {
        await onRetry();
        if (isCurrentAction(taskId, request)) {
          message = '任务已经重新加入队列';
        }
      } else {
        await onCancel();
        if (isCurrentAction(taskId, request)) message = '任务已经取消';
      }
    } catch {
      if (isCurrentAction(taskId, request)) {
        error = action === 'retry' ? '任务重试失败' : '任务取消失败';
      }
    } finally {
      if (isCurrentAction(taskId, request)) busy = false;
    }
  }

  function isCurrentAction(taskId: string, request: number): boolean {
    return activeTaskId === taskId && actionRevision === request;
  }
</script>

<aside class="panel task-detail" role="region" aria-label="任务详情">
  {#if detail}
    <PanelHeader title={taskKindLabel(detail.task.kind)}>
      {#snippet actions()}
        <StatusPill
          label={taskStateLabel(detail.task.state)}
          tone={taskStateTone(detail.task.state)}
        />
      {/snippet}
    </PanelHeader>
    <div class="detail-body">
      <KeyValueList>
        <div>
          <dt>所在队列</dt>
          <dd>{taskPriorityLabel(detail.task.priority)}</dd>
        </div>
        <div>
          <dt>错误分类</dt>
          <dd>{errorClassLabel(detail.task.error_class)}</dd>
        </div>
        <div>
          <dt>下次重试</dt>
          <dd><ReadableTime value={detail.task.next_retry_at} /></dd>
        </div>
        <div>
          <dt>任务ID</dt>
          <dd class="mono">{detail.task.id}</dd>
        </div>
      </KeyValueList>

      <div class="button-row">
        {#if ['failed', 'retry_wait'].includes(detail.task.state)}
          <Button variant="primary" {busy} onclick={() => runAction('retry')}
            >重试任务</Button
          >
        {/if}
        {#if !['completed', 'cancelled'].includes(detail.task.state)}
          <Button variant="danger" {busy} onclick={() => runAction('cancel')}
            >取消任务</Button
          >
        {/if}
      </div>

      {#if message}
        <p class="inline-message success">{message}</p>
      {/if}
      {#if error}
        <p class="inline-message error" role="alert">{error}</p>
      {/if}

      <section class="attempts">
        <h3>尝试记录</h3>
        {#each detail.attempts as attempt (attempt.attempt_number)}
          <article>
            <header>
              <strong>第{attempt.attempt_number}次</strong>
              <span>{taskStateLabel(attempt.state)}</span>
            </header>
            <p>
              {errorClassLabel(attempt.error_class)}
              {#if attempt.message}
                · {attempt.message}{/if}
            </p>
            <dl>
              <div>
                <dt>开始</dt>
                <dd><ReadableTime value={attempt.started_at} exact /></dd>
              </div>
              <div>
                <dt>耗时</dt>
                <dd>
                  {formatElapsed(attempt.started_at, attempt.finished_at)}
                </dd>
              </div>
              {#if attempt.trace_id}
                <div>
                  <dt>Trace ID</dt>
                  <dd class="mono">{attempt.trace_id}</dd>
                </div>
              {/if}
            </dl>
          </article>
        {:else}
          <p class="inline-message">还没有执行尝试</p>
        {/each}
      </section>
    </div>
  {:else}
    <EmptyState message="未选择任务" />
  {/if}
</aside>

<style>
  .task-detail {
    position: sticky;
    top: calc(var(--topbar-height) + var(--secondary-nav-height) + 18px);
  }

  .attempts dl {
    display: grid;
    gap: 0;
    margin: 0;
  }

  .attempts dl > div {
    display: grid;
    grid-template-columns: 100px minmax(0, 1fr);
    gap: 0.7rem;
    padding: 0.5rem 0;
    border-bottom: 1px solid var(--color-border);
  }

  dt {
    color: var(--color-text-3);
    font-size: 0.69rem;
  }

  dd {
    min-width: 0;
    margin: 0;
    overflow-wrap: anywhere;
    color: var(--color-text-2);
    font-size: 0.72rem;
  }

  .attempts {
    display: grid;
    gap: 0.65rem;
  }

  .attempts h3 {
    margin: 0.3rem 0 0;
    font-size: 0.82rem;
  }

  .attempts article {
    display: grid;
    gap: 0.45rem;
    padding: 0.8rem;
    border-radius: var(--radius-sm);
    background: var(--color-surface-2);
  }

  .attempts article header {
    display: flex;
    justify-content: space-between;
    gap: 0.7rem;
    font-size: 0.74rem;
  }

  .attempts article header span {
    color: var(--color-primary);
  }

  .attempts article p {
    margin: 0;
    color: var(--color-text-2);
    font-size: 0.72rem;
    line-height: 1.45;
  }
</style>
