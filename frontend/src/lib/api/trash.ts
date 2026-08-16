import { apiRequest, type ApiRequest } from './client';
import type {
  GalleryContextSelectionExpression,
  GallerySelectionExpression
} from './gallery';
import type { components } from './schema';

export type TrashWork = components['schemas']['TrashWorkSummaryDto'];
export type TrashEntry = components['schemas']['TrashEntryDto'];
export type TrashList = components['schemas']['TrashListDto'];
export type TrashCursor = components['schemas']['TrashCursorDto'];
export type TrashSummary = components['schemas']['TrashCollectionSummaryDto'];
export type PurgeAccepted = components['schemas']['PurgeAccepted'];
export type TrashPurgeState = 'pending' | 'running' | 'failed';
export type TrashFilter = components['schemas']['TrashFilter'];
export type TrashSelectionExpression =
  components['schemas']['TrashSelectionExpression'];
export type TrashSelectionRequest = components['schemas']['TrashSelectionBody'];
export type TrashSelectionProjection =
  components['schemas']['TrashSelectionDto'];

export interface TrashListOptions {
  limit?: number;
  query?: string;
  purgeStates?: TrashPurgeState[];
  cursor?: TrashCursor | null;
}

export function listTrash(
  options: TrashListOptions = {},
  request: ApiRequest = apiRequest
): Promise<TrashList> {
  const search = new URLSearchParams({
    limit: String(options.limit ?? 50)
  });
  if (options.query) search.set('query', options.query);
  for (const purgeState of options.purgeStates ?? []) {
    search.append('purge_state', purgeState);
  }
  if (options.cursor) {
    search.set('cursor_scheduled_purge_at', options.cursor.scheduled_purge_at);
    search.set('cursor_work_id', options.cursor.work_id);
  }
  return request(`/api/trash?${search.toString()}`);
}

export function moveWorkToTrash(
  workId: string,
  retentionDays: number,
  request: ApiRequest = apiRequest
): Promise<TrashEntry> {
  return request(`/api/works/${workId}/trash`, {
    method: 'POST',
    json: { retention_days: retentionDays }
  });
}

export function projectTrashSelection(
  selection: TrashSelectionRequest,
  request: ApiRequest = apiRequest
): Promise<TrashSelectionProjection> {
  return request('/api/trash/selection', {
    method: 'POST',
    json: selection
  });
}

export async function moveGalleryToTrash(
  expression: GallerySelectionExpression,
  retentionDays: number,
  request: ApiRequest = apiRequest
): Promise<number> {
  const response = await request<
    components['schemas']['MoveGalleryToTrashDto']
  >('/api/gallery/trash', {
    method: 'POST',
    json: {
      expression,
      retention_days: retentionDays
    }
  });
  return response.moved_count;
}

export async function moveGalleryContextsToTrash(
  expression: GalleryContextSelectionExpression,
  retentionDays: number,
  request: ApiRequest = apiRequest
): Promise<number> {
  const response = await request<
    components['schemas']['MoveGalleryToTrashDto']
  >('/api/gallery/contexts/trash', {
    method: 'POST',
    json: {
      expression,
      retention_days: retentionDays
    }
  });
  return response.moved_count;
}

export function restoreWork(
  workId: string,
  request: ApiRequest = apiRequest
): Promise<void> {
  return request(`/api/trash/${workId}/restore`, { method: 'POST' });
}

export function rescheduleWork(
  workId: string,
  scheduledPurgeAt: string,
  request: ApiRequest = apiRequest
): Promise<void> {
  return request(`/api/trash/${workId}/schedule`, {
    method: 'PUT',
    json: { scheduled_purge_at: scheduledPurgeAt }
  });
}

export function purgeWork(
  workId: string,
  request: ApiRequest = apiRequest
): Promise<PurgeAccepted> {
  return request(`/api/trash/${workId}/purge`, { method: 'POST' });
}

export async function restoreWorks(
  expression: TrashSelectionExpression,
  request: ApiRequest = apiRequest
): Promise<number> {
  const response = await request<components['schemas']['TrashBatchAccepted']>(
    '/api/trash/restore',
    {
      method: 'POST',
      json: { expression }
    }
  );
  return response.affected_count;
}

export async function rescheduleWorks(
  expression: TrashSelectionExpression,
  scheduledPurgeAt: string,
  request: ApiRequest = apiRequest
): Promise<number> {
  const response = await request<components['schemas']['TrashBatchAccepted']>(
    '/api/trash/schedule',
    {
      method: 'PUT',
      json: { expression, scheduled_purge_at: scheduledPurgeAt }
    }
  );
  return response.affected_count;
}

export async function purgeWorks(
  expression: TrashSelectionExpression,
  request: ApiRequest = apiRequest
): Promise<number> {
  const response = await request<components['schemas']['TrashBatchAccepted']>(
    '/api/trash/purge',
    {
      method: 'POST',
      json: { expression }
    }
  );
  return response.affected_count;
}

export function purgeAllTrash(
  request: ApiRequest = apiRequest
): Promise<components['schemas']['PurgeAllAccepted']> {
  return request('/api/trash/purge-all', { method: 'POST' });
}
