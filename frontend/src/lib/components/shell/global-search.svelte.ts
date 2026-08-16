import type { Pathname } from '$app/types';

import {
  listArtists,
  listSeries,
  listTags,
  searchGallery,
  type GalleryArtistPage,
  type GallerySearch,
  type GallerySearchPage,
  type GallerySeriesPage,
  type GalleryTagPage,
  type GalleryWork,
  type PixivAgeRating
} from '$lib/api/gallery';
import { followingAuthorAvatarUrl } from '$lib/api/following';
import {
  galleryArtistPath,
  gallerySeriesPath,
  galleryTagPath,
  galleryWorkPath
} from '$lib/gallery-routes';
import { createGalleryTextSearch } from '$lib/gallery-search';
import { LatestRequest, type RequestToken } from '$lib/latest-request';
import { searchNavigationPages, type NavigationIcon } from '$lib/navigation';

export type GlobalSearchKind = 'work' | 'artist' | 'tag' | 'series' | 'page';
export type GlobalSearchGroupKey = GlobalSearchKind;
export type GlobalSearchStatus = 'idle' | 'loading' | 'success' | 'error';

export const globalSearchGroupKeys = [
  'work',
  'artist',
  'tag',
  'series',
  'page'
] as const satisfies readonly GlobalSearchGroupKey[];

export const globalSearchKindLabels: Record<GlobalSearchKind, string> = {
  work: '作品',
  artist: '作者',
  tag: '标签',
  series: '系列',
  page: '页面'
};

interface GlobalSearchResultBase {
  key: string;
  kind: GlobalSearchKind;
  label: string;
  detail: string;
  href: Pathname;
}

export interface GlobalSearchCoverResult extends GlobalSearchResultBase {
  kind: 'work' | 'series';
  coverUrl: string | null;
  ageRating: PixivAgeRating | null;
}

export interface GlobalSearchArtistResult extends GlobalSearchResultBase {
  kind: 'artist';
  avatarUrl: string;
}

export interface GlobalSearchIconResult extends GlobalSearchResultBase {
  kind: 'tag' | 'page';
  icon: NavigationIcon | 'tag';
}

export type GlobalSearchResult =
  GlobalSearchCoverResult | GlobalSearchArtistResult | GlobalSearchIconResult;

export interface GlobalSearchGroup {
  status: GlobalSearchStatus;
  items: GlobalSearchResult[];
}

export interface GlobalSearchGateway {
  searchWorks(query: GallerySearch): Promise<GallerySearchPage>;
  searchArtists(query: string): Promise<GalleryArtistPage>;
  searchTags(query: string): Promise<GalleryTagPage>;
  searchSeries(query: string): Promise<GallerySeriesPage>;
}

const RESULT_LIMIT = 5;
const DEFAULT_DELAY_MS = 180;

const defaultGateway: GlobalSearchGateway = {
  searchWorks: searchGallery,
  searchArtists: (query) => listArtists(query, null, RESULT_LIMIT),
  searchTags: (query) => listTags(query, null, RESULT_LIMIT),
  searchSeries: (query) => listSeries(query, null, RESULT_LIMIT)
};

export class GlobalSearchSession {
  query = $state('');
  groups = $state<Record<GlobalSearchGroupKey, GlobalSearchGroup>>({
    work: emptyGroup(),
    artist: emptyGroup(),
    tag: emptyGroup(),
    series: emptyGroup(),
    page: {
      status: 'success',
      items: searchNavigationPages('')
    }
  });
  selectedKey = $state<string | null>(this.groups.page.items[0]?.key ?? null);

  private readonly requests = new LatestRequest();
  private delayTimer: ReturnType<typeof setTimeout> | null = null;
  private disposed = false;

  constructor(
    private readonly gateway: GlobalSearchGateway = defaultGateway,
    private readonly delayMs = DEFAULT_DELAY_MS
  ) {}

  get results(): GlobalSearchResult[] {
    return globalSearchGroupKeys.flatMap((key) => this.groups[key].items);
  }

  get selectedResult(): GlobalSearchResult | null {
    return (
      this.results.find((result) => result.key === this.selectedKey) ?? null
    );
  }

  search(value: string): void {
    if (this.disposed) return;

    this.clearDelay();
    this.requests.invalidate();
    this.query = value.trim();
    this.groups.page = {
      status: 'success',
      items: searchNavigationPages(this.query)
    };

    if (!this.query) {
      this.clearRemoteGroups();
      this.reconcileSelection();
      return;
    }

    for (const key of remoteGroupKeys) {
      this.groups[key] = { status: 'loading', items: [] };
    }
    this.reconcileSelection();

    const query = this.query;
    this.delayTimer = setTimeout(() => {
      this.delayTimer = null;
      this.loadArchiveGroups(query);
    }, this.delayMs);
  }

  select(key: string): void {
    if (this.results.some((result) => result.key === key)) {
      this.selectedKey = key;
    }
  }

  moveSelection(offset: number): void {
    const results = this.results;
    if (!results.length) {
      this.selectedKey = null;
      return;
    }

    const currentIndex = results.findIndex(
      (result) => result.key === this.selectedKey
    );
    const start = currentIndex < 0 ? 0 : currentIndex;
    const nextIndex = (start + offset + results.length) % results.length;
    this.selectedKey = results[nextIndex].key;
  }

  dispose(): void {
    this.disposed = true;
    this.clearDelay();
    this.requests.invalidate();
  }

  private loadArchiveGroups(query: string): void {
    const token = this.requests.begin();
    void this.loadGroup(
      'work',
      token,
      this.gateway.searchWorks(createGalleryTextSearch(query, RESULT_LIMIT)),
      (page) => page.items.map(projectWork)
    );
    void this.loadGroup(
      'artist',
      token,
      this.gateway.searchArtists(query),
      (page) => page.items.map(projectArtist)
    );
    void this.loadGroup('tag', token, this.gateway.searchTags(query), (page) =>
      page.items.map(projectTag)
    );
    void this.loadGroup(
      'series',
      token,
      this.gateway.searchSeries(query),
      (page) => page.items.map(projectSeries)
    );
  }

  private async loadGroup<T>(
    key: Exclude<GlobalSearchGroupKey, 'page'>,
    token: RequestToken,
    request: Promise<T>,
    project: (response: T) => GlobalSearchResult[]
  ): Promise<void> {
    try {
      const response = await request;
      if (!this.canPublish(token)) return;
      this.groups[key] = { status: 'success', items: project(response) };
    } catch {
      if (!this.canPublish(token)) return;
      this.groups[key] = { status: 'error', items: [] };
    }
    this.reconcileSelection();
  }

  private canPublish(token: RequestToken): boolean {
    return !this.disposed && this.requests.isCurrent(token);
  }

  private reconcileSelection(): void {
    const results = this.results;
    if (!results.some((result) => result.key === this.selectedKey)) {
      this.selectedKey = results[0]?.key ?? null;
    }
  }

  private clearRemoteGroups(): void {
    for (const key of remoteGroupKeys) this.groups[key] = emptyGroup();
  }

  private clearDelay(): void {
    if (this.delayTimer === null) return;
    clearTimeout(this.delayTimer);
    this.delayTimer = null;
  }
}

const remoteGroupKeys = ['work', 'artist', 'tag', 'series'] as const;

function emptyGroup(): GlobalSearchGroup {
  return { status: 'idle', items: [] };
}

function projectWork(work: GalleryWork): GlobalSearchCoverResult {
  return {
    key: `work:${work.id}`,
    kind: 'work',
    label: work.title,
    detail: `${work.artist_name} · Pixiv ID ${work.pixiv_work_id}`,
    href: galleryWorkPath(work.pixiv_work_id),
    coverUrl: work.cover_url,
    ageRating: work.age_rating
  };
}

function projectArtist(
  artist: GalleryArtistPage['items'][number]
): GlobalSearchArtistResult {
  return {
    key: `artist:${artist.id}`,
    kind: 'artist',
    label: artist.name,
    detail: `Pixiv ID ${artist.pixiv_artist_id} · ${artist.work_count}件作品`,
    href: galleryArtistPath(artist.pixiv_artist_id),
    avatarUrl: followingAuthorAvatarUrl(artist.pixiv_artist_id)
  };
}

function projectTag(
  tag: GalleryTagPage['items'][number]
): GlobalSearchIconResult {
  const translated = tag.tag.translation?.trim();
  const label = translated || tag.tag.original;
  const original =
    translated && translated !== tag.tag.original.trim()
      ? `原名 ${tag.tag.original} · `
      : '';
  return {
    key: `tag:${tag.tag.id}`,
    kind: 'tag',
    label,
    detail: `${original}${tag.work_count}件作品`,
    href: galleryTagPath(tag.tag.original),
    icon: 'tag'
  };
}

function projectSeries(
  series: GallerySeriesPage['items'][number]
): GlobalSearchCoverResult {
  return {
    key: `series:${series.id}`,
    kind: 'series',
    label: series.title,
    detail: `Pixiv ID ${series.pixiv_series_id} · ${series.work_count}件作品`,
    href: gallerySeriesPath(series.pixiv_series_id),
    coverUrl: series.cover_url,
    ageRating: series.cover_age_rating
  };
}
