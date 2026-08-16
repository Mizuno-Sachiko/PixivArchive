import { ApiError, apiRequest, type ApiRequest } from './client';
import type { components } from './schema';

export type FilterMode = components['schemas']['FilterMode'];
export type GallerySortField = components['schemas']['GallerySortField'];
export type SortDirection = components['schemas']['SortDirection'];
export type GalleryCursor = components['schemas']['GalleryCursor'];
export type GalleryFilter = components['schemas']['GalleryFilter'];
export type GalleryFilterGroup = components['schemas']['GalleryFilterGroup'];
export type GallerySelectionExpression =
  components['schemas']['GallerySelectionExpression'];
export type GallerySelectionProjection =
  components['schemas']['GallerySelectionProjectionDto'];
export type GalleryContextKind = components['schemas']['GalleryContextKind'];
export type GalleryContextSelectionExpression =
  components['schemas']['GalleryContextSelectionExpression'];
export type GalleryContextSelectionProjection =
  components['schemas']['GalleryContextSelectionProjectionDto'];

export type GallerySearchRequest = components['schemas']['GallerySearch'];
export type GallerySearch = GallerySearchRequest & {
  group_mode: NonNullable<GallerySearchRequest['group_mode']>;
  groups: NonNullable<GallerySearchRequest['groups']>;
  sort_field: NonNullable<GallerySearchRequest['sort_field']>;
  sort_direction: NonNullable<GallerySearchRequest['sort_direction']>;
  limit: NonNullable<GallerySearchRequest['limit']>;
};

export type GalleryTag = components['schemas']['GalleryTagDto'];
export type PixivAgeRating = components['schemas']['PixivAgeRating'];
export type GalleryWork = components['schemas']['GalleryWorkDto'];
export type GallerySearchPage = components['schemas']['GallerySearchPageDto'];
export type GalleryDerivative = components['schemas']['GalleryDerivativeDto'];
export type GalleryMediaRevision =
  components['schemas']['GalleryMediaRevisionDto'];
export type GalleryPage = components['schemas']['GalleryPageDto'];
export type UgoiraManifest = components['schemas']['UgoiraManifestDto'];
export type GalleryWorkDetail = components['schemas']['GalleryWorkDetailDto'];
export type WorkRevisionSummary =
  components['schemas']['WorkRevisionSummaryDto'];
export type ArtistDetail = components['schemas']['GalleryArtistDetailDto'];
export type TagDetail = components['schemas']['GalleryTagDetailDto'];
export type SeriesDetail = components['schemas']['GallerySeriesDetailDto'];
export type GalleryArtistPage = components['schemas']['GalleryArtistPageDto'];
export type GalleryTagPage = components['schemas']['GalleryTagPageDto'];
export type GallerySeriesPage = components['schemas']['GallerySeriesPageDto'];
export type OverviewDecoration = components['schemas']['OverviewDecorationDto'];

export function searchGallery(
  search: GallerySearch,
  request: ApiRequest = apiRequest
): Promise<GallerySearchPage> {
  return request('/api/gallery/search', {
    method: 'POST',
    json: search
  });
}

export async function countGallery(
  search: GallerySearch,
  request: ApiRequest = apiRequest
): Promise<number> {
  const response = await request<components['schemas']['GalleryCountDto']>(
    '/api/gallery/count',
    {
      method: 'POST',
      json: search
    }
  );
  return response.count;
}

export function projectGallerySelection(
  expression: GallerySelectionExpression,
  visibleWorkIds: string[],
  request: ApiRequest = apiRequest
): Promise<GallerySelectionProjection> {
  return request('/api/gallery/selection', {
    method: 'POST',
    json: {
      expression,
      visible_work_ids: visibleWorkIds
    }
  });
}

export function projectGalleryContextSelection(
  expression: GalleryContextSelectionExpression,
  visibleContextIds: string[],
  request: ApiRequest = apiRequest
): Promise<GalleryContextSelectionProjection> {
  return request('/api/gallery/contexts/selection', {
    method: 'POST',
    json: {
      expression,
      visible_context_ids: visibleContextIds
    }
  });
}

export function getWorkDetail(
  workId: string,
  request: ApiRequest = apiRequest
): Promise<GalleryWorkDetail> {
  return request(`/api/works/${workId}`);
}

export function getWorkRevisions(
  workId: string,
  request: ApiRequest = apiRequest
): Promise<WorkRevisionSummary[]> {
  return request(`/api/works/${workId}/revisions`);
}

export function workDownloadUrl(workId: string): string {
  return `/api/works/${workId}/download`;
}

export function listArtists(
  query = '',
  cursor: string | null = null,
  limit = 48,
  request: ApiRequest = apiRequest
): Promise<GalleryArtistPage> {
  const params = contextListParams(query, cursor, limit);
  return request(`/api/gallery/artists?${params}`);
}

export function getArtist(
  pixivArtistId: number,
  request: ApiRequest = apiRequest
): Promise<ArtistDetail> {
  return request(`/api/gallery/artists/${pixivArtistId}`);
}

export function listTags(
  query = '',
  cursor: string | null = null,
  limit = 48,
  request: ApiRequest = apiRequest
): Promise<GalleryTagPage> {
  const params = contextListParams(query, cursor, limit);
  return request(`/api/gallery/tags?${params}`);
}

export function getTag(
  tagName: string,
  request: ApiRequest = apiRequest
): Promise<TagDetail> {
  return request(`/api/gallery/tags/${encodeURIComponent(tagName.trim())}`);
}

export function listSeries(
  query = '',
  cursor: string | null = null,
  limit = 48,
  request: ApiRequest = apiRequest
): Promise<GallerySeriesPage> {
  const params = contextListParams(query, cursor, limit);
  return request(`/api/gallery/series?${params}`);
}

export function getSeries(
  pixivSeriesId: number,
  request: ApiRequest = apiRequest
): Promise<SeriesDetail> {
  return request(`/api/gallery/series/${pixivSeriesId}`);
}

export async function getOverviewDecorations(
  date: string,
  request: ApiRequest = apiRequest
): Promise<Array<OverviewDecoration | null>> {
  const response = await request<
    components['schemas']['OverviewDecorationsDto']
  >(`/api/gallery/overview-decorations?${new URLSearchParams({ date })}`);
  return response.items;
}

export async function shuffleOverviewDecorations(
  date: string,
  request: ApiRequest = apiRequest
): Promise<Array<OverviewDecoration | null>> {
  const response = await request<
    components['schemas']['OverviewDecorationsDto']
  >(`/api/gallery/overview-decorations?${new URLSearchParams({ date })}`, {
    method: 'POST'
  });
  return response.items;
}

export async function resolveWorkIdByPixivId(
  pixivWorkId: number,
  request: ApiRequest = apiRequest
): Promise<string | null> {
  try {
    const response = await request<
      components['schemas']['WorkIdResolutionDto']
    >(`/api/works/by-pixiv-id/${pixivWorkId}`);
    return response.work_id;
  } catch (cause) {
    if (cause instanceof ApiError && cause.status === 404) return null;
    throw cause;
  }
}

function contextListParams(
  query: string,
  cursor: string | null,
  limit: number
): URLSearchParams {
  const params = new URLSearchParams({
    limit: String(limit)
  });
  if (cursor) params.set('cursor', cursor);
  if (query.trim()) params.set('q', query.trim());
  return params;
}
