import { apiRequest, type ApiRequest } from './client';
import type { components } from './schema';

export type Task = components['schemas']['TaskDto'];
export type TaskAttempt = components['schemas']['TaskAttemptDto'];
export type TaskDetail = components['schemas']['TaskDetailDto'];
export type TaskSummary = components['schemas']['TaskSummaryDto'];
export type TaskList = components['schemas']['TaskList'];

export const TASK_LIST_LIMIT = 200;

export interface TaskApi {
  list(
    limit?: number,
    filters?: { kind?: string; errorsOnly?: boolean }
  ): Promise<TaskList>;
  get(id: string): Promise<TaskDetail>;
  retry(id: string, expectedRevision: number): Promise<Task>;
  cancel(id: string, expectedRevision: number): Promise<Task>;
}

export function createTaskApi(request: ApiRequest = apiRequest): TaskApi {
  return {
    async list(limit = TASK_LIST_LIMIT, filters = {}) {
      const search = new URLSearchParams({ limit: String(limit) });
      if (filters.kind) search.set('kind', filters.kind);
      if (filters.errorsOnly) search.set('errors_only', 'true');
      return request<TaskList>(`/api/tasks?${search}`);
    },
    get(id) {
      return request(`/api/tasks/${id}`);
    },
    retry(id, expectedRevision) {
      return request(`/api/tasks/${id}/retry`, {
        method: 'POST',
        json: { expected_revision: expectedRevision }
      });
    },
    cancel(id, expectedRevision) {
      return request(`/api/tasks/${id}/cancel`, {
        method: 'POST',
        json: { expected_revision: expectedRevision }
      });
    }
  };
}

export const taskApi = createTaskApi();
