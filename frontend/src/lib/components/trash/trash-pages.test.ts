import { describe, expect, it } from 'vitest';

import type { TrashList, TrashWork } from '$lib/api/trash';

import { loadTrashSnapshot, mergeTrashSchedules } from './trash-pages';

const firstId = '0198f64c-42a2-7374-bace-9f1c3b317fc1';
const secondId = '0198f64c-42a2-7374-bace-9f1c3b317fc2';

describe('trash page loading', () => {
  it('loads the previously visible depth before replacing the snapshot', async () => {
    const requests: Array<{ cursor?: unknown; limit?: number }> = [];
    const pages = [
      trashPage([work(firstId)], {
        scheduled_purge_at: '2026-08-20T00:00:00Z',
        work_id: firstId
      }),
      trashPage([work(secondId)], null)
    ];

    const snapshot = await loadTrashSnapshot(
      async (options) => {
        requests.push(structuredClone(options));
        return pages.shift()!;
      },
      { query: '保留', purgeStates: ['pending'] },
      2,
      1
    );

    expect(snapshot.items.map((item) => item.work_id)).toEqual([
      firstId,
      secondId
    ]);
    expect(requests).toEqual([
      { query: '保留', purgeStates: ['pending'], limit: 1 },
      {
        query: '保留',
        purgeStates: ['pending'],
        limit: 1,
        cursor: {
          scheduled_purge_at: '2026-08-20T00:00:00Z',
          work_id: firstId
        }
      }
    ]);
  });

  it('keeps dirty schedules while accepting current server values for other works', () => {
    const schedules = mergeTrashSchedules(
      [work(firstId), work(secondId)],
      {
        [firstId]: '2026-09-01T00:00:00Z',
        hidden: '2026-09-02T00:00:00Z'
      },
      new Set([firstId, 'hidden'])
    );

    expect(schedules).toEqual({
      [firstId]: '2026-09-01T00:00:00Z',
      hidden: '2026-09-02T00:00:00Z',
      [secondId]: '2026-08-19T12:00:00Z'
    });
  });
});

function trashPage(
  items: TrashWork[],
  nextCursor: TrashList['next_cursor']
): TrashList {
  return {
    items,
    next_cursor: nextCursor,
    summary: {
      total_count: 2,
      logical_bytes: 2048,
      estimated_reclaimable_bytes: 1024
    },
    all_summary: {
      total_count: 2,
      logical_bytes: 2048,
      estimated_reclaimable_bytes: 1024
    }
  };
}

function work(workId: string): TrashWork {
  return {
    work_id: workId,
    pixiv_work_id: 9911,
    title: workId,
    artist_name: 'Archive',
    page_count: 1,
    previous_collection_state: 'collected',
    trashed_at: '2026-07-20T12:00:00Z',
    scheduled_purge_at: '2026-08-19T12:00:00Z',
    purge_state: 'pending',
    purge_attempts: 0,
    failure_message: null,
    capabilities: {
      can_restore: true,
      can_reschedule: true,
      blocked_reason: null
    },
    estimated_release_bytes: 1024
  };
}
