<script lang="ts">
  import type { Task } from '$lib/api/tasks';
  import DataTable from '$lib/components/ui/DataTable.svelte';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import ReadableTime from '$lib/components/ui/ReadableTime.svelte';
  import StatusPill from '$lib/components/ui/StatusPill.svelte';
  import {
    taskKindLabel,
    taskPriorityLabel,
    taskStateLabel
  } from '$lib/labels';
  import { taskStateTone } from '$lib/task-status';

  interface Props {
    items: Task[];
    selectedId: string | null;
    onSelect: (id: string) => void;
  }

  let { items, selectedId, onSelect }: Props = $props();

  function handleRowKeydown(event: KeyboardEvent, id: string): void {
    if (event.key !== 'Enter' && event.key !== ' ') return;
    event.preventDefault();
    onSelect(id);
  }
</script>

<DataTable ariaLabel="任务运行记录">
  <thead>
    <tr>
      <th>任务</th>
      <th>队列</th>
      <th>状态</th>
      <th>尝试</th>
      <th>更新时间</th>
    </tr>
  </thead>
  <tbody>
    {#each items as task (task.id)}
      <tr
        role="button"
        class:selected={selectedId === task.id}
        tabindex="0"
        aria-label={`${taskKindLabel(task.kind)} ${task.id}`}
        onclick={() => onSelect(task.id)}
        onkeydown={(event) => handleRowKeydown(event, task.id)}
      >
        <td><strong class="row-title">{taskKindLabel(task.kind)}</strong></td>
        <td>{taskPriorityLabel(task.priority)}</td>
        <td>
          <StatusPill
            label={taskStateLabel(task.state)}
            tone={taskStateTone(task.state)}
          />
        </td>
        <td>{task.attempts}</td>
        <td><ReadableTime value={task.updated_at} /></td>
      </tr>
    {:else}
      <tr>
        <td colspan="5">
          <EmptyState message="当前视图没有任务" />
        </td>
      </tr>
    {/each}
  </tbody>
</DataTable>

<style>
  tbody tr {
    cursor: pointer;
  }

  tbody tr:focus-visible {
    outline: 2px solid var(--color-primary);
    outline-offset: -2px;
  }

  .row-title {
    color: var(--color-text-1);
    font-size: 0.74rem;
  }
</style>
