import { describe, expect, it } from 'vitest';

import type { TrashWork } from '$lib/api/trash';

import {
  removeWorkFromTrashReturn,
  saveTrashReturn,
  takeTrashReturn
} from './trash-return';

const work = {
  work_id: '0198f64c-42a2-7374-bace-9f1c3b317fc1',
  estimated_release_bytes: 1024
} as TrashWork;

describe('trash return snapshot', () => {
  it('removes a restored work while preserving the remaining view state', () => {
    saveTrashReturn({
      route: '/system/trash',
      query: '草稿',
      purgeStates: ['pending'],
      appliedQuery: '已应用',
      appliedPurgeStates: ['pending'],
      items: [work],
      nextCursor: null,
      summary: {
        total_count: 1,
        logical_bytes: 2048,
        estimated_reclaimable_bytes: 1536
      },
      allSummary: {
        total_count: 2,
        logical_bytes: 4096,
        estimated_reclaimable_bytes: 3072
      },
      schedules: { [work.work_id]: '2026-08-20T00:00:00Z' },
      dirtyScheduleIds: [work.work_id],
      viewport: { scrollY: 420, anchorId: work.work_id, anchorOffset: 32 }
    });

    removeWorkFromTrashReturn(work.work_id);
    const restored = takeTrashReturn();

    expect(restored?.items).toEqual([]);
    expect(restored?.summary).toEqual({
      total_count: 1,
      logical_bytes: 2048,
      estimated_reclaimable_bytes: 1536
    });
    expect(restored?.allSummary).toEqual({
      total_count: 2,
      logical_bytes: 4096,
      estimated_reclaimable_bytes: 3072
    });
    expect(restored?.schedules).toEqual({});
    expect(restored?.dirtyScheduleIds).toEqual([]);
    expect(restored?.query).toBe('草稿');
    expect(restored?.viewport.scrollY).toBe(420);
  });
});
