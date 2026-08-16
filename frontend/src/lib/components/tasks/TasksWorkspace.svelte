<script lang="ts">
  import { TASK_LIST_LIMIT } from '$lib/api/tasks';
  import RecentListCount from '$lib/components/ui/RecentListCount.svelte';
  import { onMount } from 'svelte';

  import {
    AppEventRefreshCoordinator,
    currentAppEventVersion
  } from '$lib/app-event-refresh';
  import RetryMessage from '$lib/components/ui/RetryMessage.svelte';
  import MetricStrip from '$lib/components/ui/MetricStrip.svelte';
  import PageHeader from '$lib/components/ui/PageHeader.svelte';
  import PanelHeader from '$lib/components/ui/PanelHeader.svelte';
  import { formatCount, formatExactCount } from '$lib/format';
  import { tasksStore, type TaskView } from '$lib/stores/tasks.svelte';

  import TaskDetailPanel from './TaskDetailPanel.svelte';
  import TaskTable from './TaskTable.svelte';

  interface Props {
    initialView?: TaskView;
  }

  const taskResources = ['job'] as const;
  let { initialView = 'all' }: Props = $props();
  const taskRefresh = new AppEventRefreshCoordinator(() => tasksStore.load());

  onMount(() => {
    tasksStore.reset();
    tasksStore.view = initialView;
    taskRefresh.start(currentAppEventVersion(taskResources));
    return () => {
      taskRefresh.dispose();
      tasksStore.reset();
    };
  });

  $effect(() => {
    taskRefresh.observe(currentAppEventVersion(taskResources));
  });
</script>

<section class="workspace-page">
  <PageHeader title="任务与运行记录" />

  <MetricStrip
    items={[
      {
        label: '运行中任务',
        value: formatCount(tasksStore.summary.running),
        title: formatExactCount(tasksStore.summary.running)
      },
      {
        label: '正在等待',
        value: formatCount(tasksStore.summary.waiting),
        title: formatExactCount(tasksStore.summary.waiting)
      },
      {
        label: '需要处理',
        value: formatCount(tasksStore.summary.requires_attention),
        title: formatExactCount(tasksStore.summary.requires_attention)
      }
    ]}
  />

  {#if tasksStore.error}
    <RetryMessage
      message={tasksStore.error}
      busy={tasksStore.loading}
      onRetry={() => taskRefresh.retry()}
    />
  {/if}

  <div class="workspace-layout">
    <section class="panel">
      <PanelHeader title="任务队列">
        {#snippet actions()}
          <RecentListCount
            count={tasksStore.visible().length}
            limit={TASK_LIST_LIMIT}
          />
        {/snippet}
      </PanelHeader>
      <TaskTable
        items={tasksStore.visible()}
        selectedId={tasksStore.selectedId}
        onSelect={(id) => tasksStore.select(id)}
      />
    </section>

    <TaskDetailPanel
      detail={tasksStore.selected}
      onRetry={() => tasksStore.retry()}
      onCancel={() => tasksStore.cancel()}
    />
  </div>
</section>
