import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type {
  GalleryArtistPage,
  GallerySearch,
  GallerySearchPage,
  GallerySeriesPage,
  GalleryTagPage,
  GalleryWork
} from '$lib/api/gallery';

import {
  GlobalSearchSession,
  type GlobalSearchGateway
} from './global-search.svelte';

describe('global search session', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it('shows pages without requesting archive groups for an empty query', async () => {
    const calls: string[] = [];
    const session = new GlobalSearchSession(recordingGateway(calls), 20);

    session.search('   ');
    await vi.runAllTimersAsync();

    expect(calls).toEqual([]);
    expect(session.groups.page.items.map((result) => result.label)).toEqual([
      '系统概况',
      '全部作品',
      '订阅计划',
      '规则工作台',
      '运行记录',
      'Pixiv账户'
    ]);
    expect(session.groups.work.status).toBe('idle');
    expect(session.selectedResult?.kind).toBe('page');
  });

  it('maps all archive responses into the common result order', async () => {
    const workRequests: GallerySearch[] = [];
    const session = new GlobalSearchSession(
      gateway({
        searchWorks: async (query) => {
          workRequests.push(structuredClone(query));
          return {
            items: [galleryWork('work-1', '雨后的夜空')],
            next_cursor: null
          };
        },
        searchArtists: async () => ({
          items: [artist('artist-1', '夜空みつき')],
          next_cursor: null,
          total: 1
        }),
        searchTags: async () => ({
          items: [tag('tag-1', 'yozora', '夜空')],
          next_cursor: null,
          total: 1
        }),
        searchSeries: async () => ({
          items: [series('series-1', '星屑観測記')],
          next_cursor: null,
          total: 1
        })
      }),
      20
    );

    session.search('夜空');
    await vi.runAllTimersAsync();

    expect(workRequests).toEqual([
      {
        group_mode: 'all',
        groups: [
          {
            mode: 'all',
            filters: [
              {
                type: 'text',
                field: 'any',
                operator: 'contains',
                value: '夜空'
              }
            ]
          }
        ],
        sort_field: 'pixiv_id',
        sort_direction: 'descending',
        limit: 5
      }
    ]);
    expect(session.results.map((result) => result.kind)).toEqual([
      'work',
      'artist',
      'tag',
      'series'
    ]);
    expect(session.groups.tag.items[0]).toMatchObject({
      label: '夜空',
      detail: '原名 yozora · 9件作品',
      href: '/gallery/tags/yozora'
    });
    expect(session.groups.artist.items[0]).toMatchObject({
      label: '夜空みつき',
      avatarUrl: '/api/following/authors/3249187/avatar'
    });
    expect(session.groups.artist.items[0]).not.toHaveProperty('coverUrl');
  });

  it('does not repeat a tag original name when it matches the translation', async () => {
    const session = new GlobalSearchSession(
      gateway({
        searchTags: async () => ({
          items: [tag('tag-1', '星空', '星空')],
          next_cursor: null,
          total: 1
        })
      }),
      20
    );

    session.search('星空');
    await vi.runAllTimersAsync();

    expect(session.groups.tag.items[0]).toMatchObject({
      label: '星空',
      detail: '9件作品'
    });
  });

  it('keeps successful groups and pages when one endpoint fails', async () => {
    const session = new GlobalSearchSession(
      gateway({
        searchWorks: async () => {
          throw new Error('work search unavailable');
        },
        searchArtists: async () => ({
          items: [artist('artist-1', '收藏家')],
          next_cursor: null,
          total: 1
        })
      }),
      20
    );

    session.search('收藏');
    await vi.runAllTimersAsync();

    expect(session.groups.work.status).toBe('error');
    expect(session.groups.artist.items).toHaveLength(1);
    expect(session.groups.page.items.map((result) => result.label)).toContain(
      '收藏'
    );
    expect(session.results.map((result) => result.kind)).toEqual([
      'artist',
      'page'
    ]);
  });

  it('does not let a superseded response replace the current query', async () => {
    const first = deferred<GallerySearchPage>();
    let workRequest = 0;
    const session = new GlobalSearchSession(
      gateway({
        searchWorks: async () => {
          workRequest += 1;
          return workRequest === 1
            ? first.promise
            : { items: [galleryWork('new', '新结果')], next_cursor: null };
        }
      }),
      20
    );

    session.search('old');
    await vi.advanceTimersByTimeAsync(20);
    session.search('new');
    await vi.advanceTimersByTimeAsync(20);
    await settle();

    first.resolve({ items: [galleryWork('old', '旧结果')], next_cursor: null });
    await settle();

    expect(session.groups.work.items.map((result) => result.label)).toEqual([
      '新结果'
    ]);
  });

  it('keeps selection by result identity while groups finish out of order', async () => {
    const works = deferred<GallerySearchPage>();
    const artists = deferred<GalleryArtistPage>();
    const session = new GlobalSearchSession(
      gateway({
        searchWorks: () => works.promise,
        searchArtists: () => artists.promise
      }),
      20
    );

    session.search('unmatched-page-query');
    await vi.advanceTimersByTimeAsync(20);

    artists.resolve({
      items: [artist('artist-1', '先返回的作者')],
      next_cursor: null,
      total: 1
    });
    await settle();
    expect(session.selectedResult?.kind).toBe('artist');

    works.resolve({
      items: [galleryWork('work-1', '后来返回的作品')],
      next_cursor: null
    });
    await settle();
    expect(session.selectedResult?.kind).toBe('artist');

    session.moveSelection(-1);
    expect(session.selectedResult?.kind).toBe('work');
  });

  it('ignores a pending response after disposal', async () => {
    const works = deferred<GallerySearchPage>();
    const session = new GlobalSearchSession(
      gateway({ searchWorks: () => works.promise }),
      20
    );

    session.search('night');
    await vi.advanceTimersByTimeAsync(20);
    session.dispose();
    works.resolve({
      items: [galleryWork('work-1', '不应显示')],
      next_cursor: null
    });
    await settle();

    expect(session.groups.work.items).toEqual([]);
  });
});

function gateway(
  overrides: Partial<GlobalSearchGateway> = {}
): GlobalSearchGateway {
  return {
    searchWorks: async () => ({ items: [], next_cursor: null }),
    searchArtists: async () => ({ items: [], next_cursor: null, total: 0 }),
    searchTags: async () => ({ items: [], next_cursor: null, total: 0 }),
    searchSeries: async () => ({ items: [], next_cursor: null, total: 0 }),
    ...overrides
  };
}

function recordingGateway(calls: string[]): GlobalSearchGateway {
  return gateway({
    searchWorks: async () => {
      calls.push('work');
      return { items: [], next_cursor: null };
    },
    searchArtists: async () => {
      calls.push('artist');
      return { items: [], next_cursor: null, total: 0 };
    },
    searchTags: async () => {
      calls.push('tag');
      return { items: [], next_cursor: null, total: 0 };
    },
    searchSeries: async () => {
      calls.push('series');
      return { items: [], next_cursor: null, total: 0 };
    }
  });
}

function galleryWork(id: string, title: string): GalleryWork {
  return {
    age_rating: 'all_age',
    ai_generated: false,
    artist_id: `artist-${id}`,
    artist_name: 'Mika',
    bookmark_count: 42,
    bookmark_id: null,
    bookmarked_by_current_account: false,
    collection_state: 'collected',
    comment_count: 2,
    cover_available: true,
    cover_height: 900,
    cover_url: `/covers/${id}`,
    cover_width: 640,
    description: null,
    id,
    like_count: 20,
    local_updated_at: '2026-08-14T00:00:00Z',
    media_kind: 'source_image',
    page_count: 1,
    pixiv_artist_id: 3249187,
    pixiv_published_at: '2026-08-13T00:00:00Z',
    pixiv_updated_at: null,
    pixiv_work_id: id === 'old' ? 120000 : 120001,
    series_id: null,
    series_title: null,
    source_state: 'present',
    tags: [],
    title,
    view_count: 100,
    work_kind: 'illustration'
  };
}

function artist(id: string, name: string): GalleryArtistPage['items'][number] {
  return {
    account_name: 'yozora',
    cover_age_rating: 'all_age',
    cover_height: 900,
    cover_url: `/covers/${id}`,
    cover_width: 640,
    id,
    name,
    pixiv_artist_id: 3249187,
    work_count: 128
  };
}

function tag(
  id: string,
  original: string,
  translation: string | null
): GalleryTagPage['items'][number] {
  return {
    cover_age_rating: 'all_age',
    cover_height: 900,
    cover_url: `/covers/${id}`,
    cover_width: 640,
    tag: { id, original, translation },
    work_count: 9
  };
}

function series(id: string, title: string): GallerySeriesPage['items'][number] {
  return {
    cover_age_rating: 'all_age',
    cover_height: 900,
    cover_url: `/covers/${id}`,
    cover_width: 640,
    id,
    pixiv_artist_id: 3249187,
    pixiv_series_id: 923804,
    title,
    work_count: 12
  };
}

function deferred<T>(): {
  promise: Promise<T>;
  resolve: (value: T) => void;
} {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

async function settle(): Promise<void> {
  await Promise.resolve();
  await Promise.resolve();
}
