import {
  TASK_LIST_LIMIT,
  taskApi,
  type Task,
  type TaskDetail,
  type TaskSummary
} from '$lib/api/tasks';
import { LatestRequest } from '$lib/latest-request';

export type TaskView = 'all' | 'downloads' | 'errors';

class TasksStore {
  items = $state<Task[]>([]);
  summary = $state<TaskSummary>({
    total: 0,
    running: 0,
    waiting: 0,
    requires_attention: 0
  });
  selectedId = $state<string | null>(null);
  selected = $state<TaskDetail | null>(null);
  view = $state<TaskView>('all');
  loading = $state(false);
  error = $state('');

  private readonly loadRequests = new LatestRequest();
  private readonly selectionRequests = new LatestRequest();

  visible(): Task[] {
    if (this.view === 'errors') {
      return this.items.filter((task) =>
        ['failed', 'waiting_account'].includes(task.state)
      );
    }
    if (this.view === 'downloads') {
      return this.items.filter((task) => task.kind === 'download_media');
    }
    return this.items;
  }

  async load(): Promise<boolean> {
    const request = this.loadRequests.begin();
    const view = this.view;
    this.loading = true;
    try {
      const response = await taskApi.list(
        TASK_LIST_LIMIT,
        view === 'downloads'
          ? { kind: 'download_media' }
          : view === 'errors'
            ? { errorsOnly: true }
            : {}
      );
      if (!this.loadRequests.isCurrent(request)) return false;
      this.items = response.items;
      this.summary = response.summary;
      this.error = '';
      if (
        this.selectedId &&
        !this.items.some((task) => task.id === this.selectedId)
      ) {
        this.selectionRequests.invalidate();
        this.selectedId = null;
        this.selected = null;
      }
      return true;
    } catch {
      if (!this.loadRequests.isCurrent(request)) return false;
      this.error = '任务列表暂时无法读取';
      return false;
    } finally {
      if (this.loadRequests.isCurrent(request)) this.loading = false;
    }
  }

  async select(id: string): Promise<void> {
    const request = this.selectionRequests.begin();
    this.selectedId = id;
    try {
      const selected = await taskApi.get(id);
      if (!this.selectionRequests.isCurrent(request) || this.selectedId !== id)
        return;
      this.selected = selected;
      this.error = '';
    } catch {
      if (!this.selectionRequests.isCurrent(request) || this.selectedId !== id)
        return;
      this.error = '任务详情暂时无法读取';
    }
  }

  async retry(): Promise<void> {
    if (!this.selected) return;
    const taskId = this.selected.task.id;
    const updated = await taskApi.retry(taskId, this.selected.task.revision);
    this.replace(updated);
    if (this.selectedId === taskId) {
      await this.select(updated.id);
    }
  }

  async cancel(): Promise<void> {
    if (!this.selected) return;
    const taskId = this.selected.task.id;
    const updated = await taskApi.cancel(taskId, this.selected.task.revision);
    this.replace(updated);
    if (this.selectedId === taskId) {
      await this.select(updated.id);
    }
  }

  reset(): void {
    this.loadRequests.invalidate();
    this.selectionRequests.invalidate();
    this.items = [];
    this.summary = {
      total: 0,
      running: 0,
      waiting: 0,
      requires_attention: 0
    };
    this.selectedId = null;
    this.selected = null;
    this.view = 'all';
    this.loading = false;
    this.error = '';
  }

  private replace(updated: Task): void {
    this.items = this.items.map((task) =>
      task.id === updated.id ? updated : task
    );
  }
}

export const tasksStore = new TasksStore();
