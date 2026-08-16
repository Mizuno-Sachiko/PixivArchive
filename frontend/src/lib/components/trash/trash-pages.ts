import {
  listTrash,
  type TrashCursor,
  type TrashList,
  type TrashListOptions,
  type TrashPurgeState,
  type TrashWork
} from '$lib/api/trash';

export interface TrashPageFilter {
  query: string;
  purgeStates: TrashPurgeState[];
}

type TrashPageLoader = (options: TrashListOptions) => Promise<TrashList>;

export async function loadTrashSnapshot(
  loadPage: TrashPageLoader = listTrash,
  filter: TrashPageFilter,
  minimumItems: number,
  pageSize = 50
): Promise<TrashList> {
  const items: TrashWork[] = [];
  let cursor: TrashCursor | null = null;
  let latest: TrashList;

  do {
    latest = await loadPage({
      query: filter.query,
      purgeStates: filter.purgeStates,
      limit: pageSize,
      ...(cursor ? { cursor } : {})
    });
    items.push(...latest.items);
    cursor = latest.next_cursor ?? null;
  } while (items.length < minimumItems && cursor !== null);

  return { ...latest, items, next_cursor: cursor };
}

export function mergeTrashSchedules(
  items: TrashWork[],
  current: Record<string, string>,
  dirtyWorkIds: ReadonlySet<string>
): Record<string, string> {
  const schedules = Object.fromEntries(
    Object.entries(current).filter(([workId]) => dirtyWorkIds.has(workId))
  );
  for (const item of items) {
    schedules[item.work_id] = dirtyWorkIds.has(item.work_id)
      ? current[item.work_id]
      : item.scheduled_purge_at;
  }
  return schedules;
}
