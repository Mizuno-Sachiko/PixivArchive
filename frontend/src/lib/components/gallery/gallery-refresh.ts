import { ResourceRefreshCoordinator } from '$lib/resource-refresh';

export interface GalleryResourceVersions {
  work: number;
  pixivBookmark: number;
  pixivAccount: number;
  snapshot: number;
}

const DEFAULT_REFRESH_DELAY = 500;

export function refreshVisibleItems<T extends { id: string }>(
  current: readonly T[],
  refreshed: readonly T[],
  update: (current: T, refreshed: T) => T = (_, latest) => latest
): T[] {
  const refreshedById = new Map(refreshed.map((item) => [item.id, item]));
  return current.flatMap((item) => {
    const latest = refreshedById.get(item.id);
    return latest ? [update(item, latest)] : [];
  });
}

export class GalleryRefreshCoordinator extends ResourceRefreshCoordinator<GalleryResourceVersions> {
  constructor(
    initial: GalleryResourceVersions,
    refresh: () => Promise<boolean>,
    delay = DEFAULT_REFRESH_DELAY
  ) {
    super(initial, refresh, {
      sameVersion,
      mergeVersions: newestVersion,
      delay
    });
  }
}

function sameVersion(
  left: GalleryResourceVersions,
  right: GalleryResourceVersions
): boolean {
  return (
    left.work === right.work &&
    left.pixivBookmark === right.pixivBookmark &&
    left.pixivAccount === right.pixivAccount &&
    left.snapshot === right.snapshot
  );
}

function newestVersion(
  left: GalleryResourceVersions,
  right: GalleryResourceVersions
): GalleryResourceVersions {
  return {
    work: Math.max(left.work, right.work),
    pixivBookmark: Math.max(left.pixivBookmark, right.pixivBookmark),
    pixivAccount: Math.max(left.pixivAccount, right.pixivAccount),
    snapshot: Math.max(left.snapshot, right.snapshot)
  };
}
