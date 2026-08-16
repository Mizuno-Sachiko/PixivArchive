import type { GalleryFilter, GallerySearch } from '$lib/api/gallery';
import { parseSourceId } from '$lib/gallery-routes';

export function createGalleryQuery(): GallerySearch {
  return {
    group_mode: 'all',
    groups: [],
    sort_field: 'pixiv_id',
    sort_direction: 'descending',
    limit: 60
  };
}

export function galleryTextFilter(value: string): GalleryFilter | null {
  const query = value.trim();
  if (!query) return null;

  const pixivWorkId = parseSourceId(query);
  return pixivWorkId === null
    ? {
        type: 'text',
        field: 'any',
        operator: 'contains',
        value: query
      }
    : { type: 'pixiv_work_id', value: pixivWorkId };
}

export function createGalleryTextSearch(
  value: string,
  limit: number
): GallerySearch {
  const filter = galleryTextFilter(value);
  return {
    ...createGalleryQuery(),
    groups: filter ? [{ mode: 'all', filters: [filter] }] : [],
    limit
  };
}
