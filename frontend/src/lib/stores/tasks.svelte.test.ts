import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { Task, TaskDetail } from '$lib/api/tasks';

const taskApi = vi.hoisted(() => ({
  list: vi.fn(),
  get: vi.fn(),
  retry: vi.fn(),
  cancel: vi.fn()
}));

vi.mock('$lib/api/tasks', async () => {
  const actual =
    await vi.importActual<typeof import('$lib/api/tasks')>('$lib/api/tasks');
  return { ...actual, taskApi };
});

import { tasksStore } from './tasks.svelte';

const firstTaskId = '0198f652-0000-7000-8000-000000000011';
const secondTaskId = '0198f652-0000-7000-8000-000000000014';

describe('tasks state', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    tasksStore.reset();
  });

  it('keeps the latest selection when an older detail request resolves last', async () => {
    const firstRequest = deferred<TaskDetail>();
    const secondRequest = deferred<TaskDetail>();
    taskApi.get.mockImplementation((id: string) => {
      if (id === firstTaskId) return firstRequest.promise;
      if (id === secondTaskId) return secondRequest.promise;
      throw new Error(`unexpected task ${id}`);
    });

    const firstSelection = tasksStore.select(firstTaskId);
    const secondSelection = tasksStore.select(secondTaskId);

    secondRequest.resolve(detail(secondTaskId, 'waiting_storage'));
    await secondSelection;
    firstRequest.resolve(detail(firstTaskId, 'cancelled'));
    await firstSelection;

    expect(tasksStore.selectedId).toBe(secondTaskId);
    expect(tasksStore.selected?.task.id).toBe(secondTaskId);
  });

  it('does not reselect a cancelled task after the user selects another task', async () => {
    const cancelRequest = deferred<Task>();
    const secondRequest = deferred<TaskDetail>();
    taskApi.get.mockImplementation((id: string) => {
      if (id === firstTaskId) {
        return Promise.resolve(detail(firstTaskId, 'waiting_account'));
      }
      if (id === secondTaskId) return secondRequest.promise;
      throw new Error(`unexpected task ${id}`);
    });
    taskApi.cancel.mockReturnValue(cancelRequest.promise);
    await tasksStore.select(firstTaskId);

    const cancellation = tasksStore.cancel();
    const secondSelection = tasksStore.select(secondTaskId);
    secondRequest.resolve(detail(secondTaskId, 'waiting_storage'));
    await secondSelection;
    cancelRequest.resolve({
      ...detail(firstTaskId, 'cancelled').task,
      revision: 4
    });
    await cancellation;

    expect(tasksStore.selectedId).toBe(secondTaskId);
    expect(tasksStore.selected?.task.id).toBe(secondTaskId);
  });

  it('requests the latest 200 items inside the selected task view', async () => {
    taskApi.list.mockResolvedValue({
      items: [],
      summary: {
        total: 1_234_567,
        running: 2,
        waiting: 8,
        requires_attention: 3
      }
    });
    tasksStore.view = 'downloads';
    expect(await tasksStore.load()).toBe(true);
    expect(taskApi.list).toHaveBeenCalledWith(200, {
      kind: 'download_media'
    });

    tasksStore.view = 'errors';
    expect(await tasksStore.load()).toBe(true);
    expect(taskApi.list).toHaveBeenLastCalledWith(200, { errorsOnly: true });
    expect(tasksStore.summary.total).toBe(1_234_567);
  });

  it('keeps the newest list when an older refresh resolves last', async () => {
    const oldRequest = deferred<{
      items: Task[];
      summary: {
        total: number;
        running: number;
        waiting: number;
        requires_attention: number;
      };
    }>();
    const newRequest = deferred<{
      items: Task[];
      summary: {
        total: number;
        running: number;
        waiting: number;
        requires_attention: number;
      };
    }>();
    taskApi.list
      .mockReturnValueOnce(oldRequest.promise)
      .mockReturnValueOnce(newRequest.promise);

    const oldLoad = tasksStore.load();
    const newLoad = tasksStore.load();
    newRequest.resolve({
      items: [detail(secondTaskId, 'running').task],
      summary: { total: 2, running: 1, waiting: 0, requires_attention: 0 }
    });
    expect(await newLoad).toBe(true);
    oldRequest.resolve({
      items: [detail(firstTaskId, 'cancelled').task],
      summary: { total: 1, running: 0, waiting: 0, requires_attention: 0 }
    });
    expect(await oldLoad).toBe(false);

    expect(tasksStore.items.map((task) => task.id)).toEqual([secondTaskId]);
    expect(tasksStore.summary.total).toBe(2);
  });

  it('reports a failed list refresh without replacing the current tasks', async () => {
    const current = detail(firstTaskId, 'running').task;
    taskApi.list.mockResolvedValueOnce({
      items: [current],
      summary: { total: 1, running: 1, waiting: 0, requires_attention: 0 }
    });
    expect(await tasksStore.load()).toBe(true);

    taskApi.list.mockRejectedValueOnce(new Error('temporarily unavailable'));
    expect(await tasksStore.load()).toBe(false);
    expect(tasksStore.items).toEqual([current]);
    expect(tasksStore.error).toBe('任务列表暂时无法读取');
  });
});

function detail(id: string, state: string): TaskDetail {
  return {
    task: {
      id,
      priority: 'background_maintenance',
      kind: 'generate_derivative',
      state,
      attempts: 1,
      available_at: '2026-07-30T03:00:00Z',
      error_class: null,
      retryable: null,
      next_retry_at: null,
      revision: 3,
      created_at: '2026-07-30T02:00:00Z',
      updated_at: '2026-07-30T02:30:00Z'
    },
    attempts: [
      {
        attempt_number: 1,
        state,
        started_at: '2026-07-30T02:00:00Z',
        finished_at: '2026-07-30T02:00:01Z',
        error_class: null,
        retryable: null,
        message: null,
        trace_id: null
      }
    ]
  };
}

function deferred<T>(): {
  promise: Promise<T>;
  resolve: (value: T) => void;
} {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((next) => {
    resolve = next;
  });
  return { promise, resolve };
}
