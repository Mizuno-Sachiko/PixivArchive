import { describe, expect, it, vi } from 'vitest';

import type { ApiRequest } from './client';
import { listTrash, purgeWorks, rescheduleWorks, restoreWorks } from './trash';

const WORK_ID = '0198f64c-42a2-7374-bace-9f1c3b317fc1';

describe('trash API', () => {
  it('sends every selected cleanup state as a repeated query parameter', async () => {
    const { request, requestMock } = mockedRequest({});

    await listTrash({ purgeStates: ['pending', 'failed'] }, request);

    const [path] = requestMock.mock.calls[0] as [string];
    const url = new URL(path, 'http://pixivarchive.local');
    expect(url.searchParams.getAll('purge_state')).toEqual([
      'pending',
      'failed'
    ]);
  });

  it('sends a fixed-filter expression for restore', async () => {
    const { request, requestMock } = mockedRequest({ affected_count: 0 });
    const expression = {
      filter: { query: null, purge_states: [] },
      base_selected: false,
      exception_work_ids: []
    };

    await expect(restoreWorks(expression, request)).resolves.toBe(0);
    expect(requestMock).toHaveBeenCalledWith('/api/trash/restore', {
      method: 'POST',
      json: { expression }
    });
  });

  it('sends an unbounded complete selection expression for purge', async () => {
    const { request, requestMock } = mockedRequest({ affected_count: 1200 });
    const expression = {
      filter: { query: '待清理', purge_states: ['failed'] },
      base_selected: true,
      exception_work_ids: [WORK_ID]
    };

    await expect(purgeWorks(expression, request)).resolves.toBe(1200);
    expect(requestMock).toHaveBeenCalledWith('/api/trash/purge', {
      method: 'POST',
      json: { expression }
    });
  });

  it('sends the same expression with a new cleanup time', async () => {
    const { request, requestMock } = mockedRequest({ affected_count: 8 });
    const expression = {
      filter: { query: null, purge_states: [] },
      base_selected: false,
      exception_work_ids: [WORK_ID]
    };

    await expect(
      rescheduleWorks(expression, '2026-08-19T12:00:00Z', request)
    ).resolves.toBe(8);
    expect(requestMock).toHaveBeenCalledWith('/api/trash/schedule', {
      method: 'PUT',
      json: {
        expression,
        scheduled_purge_at: '2026-08-19T12:00:00Z'
      }
    });
  });
});

function mockedRequest(response: unknown): {
  request: ApiRequest;
  requestMock: ReturnType<typeof vi.fn>;
} {
  const requestMock = vi.fn();
  const request: ApiRequest = async <T>(
    ...arguments_: Parameters<ApiRequest>
  ) => {
    requestMock(...arguments_);
    return response as T;
  };
  return { request, requestMock };
}
