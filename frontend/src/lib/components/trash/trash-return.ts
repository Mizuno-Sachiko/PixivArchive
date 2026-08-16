import type {
  TrashCursor,
  TrashPurgeState,
  TrashSummary,
  TrashWork
} from '$lib/api/trash';
import type { GalleryViewportSnapshot } from '$lib/stores/gallery-return';

export interface TrashReturnSnapshot {
  route: '/system/trash';
  query: string;
  purgeStates: TrashPurgeState[];
  appliedQuery: string;
  appliedPurgeStates: TrashPurgeState[];
  items: TrashWork[];
  nextCursor: TrashCursor | null;
  summary: TrashSummary;
  allSummary: TrashSummary;
  schedules: Record<string, string>;
  dirtyScheduleIds: string[];
  viewport: GalleryViewportSnapshot;
}

let snapshot: TrashReturnSnapshot | null = null;

export function saveTrashReturn(next: TrashReturnSnapshot): void {
  snapshot = next;
}

export function takeTrashReturn(): TrashReturnSnapshot | null {
  const current = snapshot;
  snapshot = null;
  return current;
}

export function removeWorkFromTrashReturn(workId: string): void {
  if (!snapshot) return;
  const item = snapshot.items.find((entry) => entry.work_id === workId);
  if (!item) return;
  snapshot = {
    ...snapshot,
    items: snapshot.items.filter((entry) => entry.work_id !== workId),
    schedules: Object.fromEntries(
      Object.entries(snapshot.schedules).filter(([id]) => id !== workId)
    ),
    dirtyScheduleIds: snapshot.dirtyScheduleIds.filter((id) => id !== workId)
  };
}
