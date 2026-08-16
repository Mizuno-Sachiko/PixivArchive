import { describe, expect, it, vi } from 'vitest';

import type {
  GalleryContextSelectionExpression,
  GallerySearch,
  GallerySearchPage,
  GalleryWork
} from '$lib/api/gallery';

import {
  GalleryContextSelectionSession,
  GallerySearchSession,
  GallerySelectionSession
} from './gallery-sessions.svelte';

const query: GallerySearch = {
  group_mode: 'all',
  groups: [],
  sort_field: 'pixiv_id',
  sort_direction: 'descending',
  limit: 48
};

describe('gallery sessions', () => {
  it('does not let an old search replace a newer reset', async () => {
    const first = deferred<GallerySearchPage>();
    const second = deferred<GallerySearchPage>();
    let request = 0;
    const session = new GallerySearchSession(
      { appliedQuery: query },
      {
        search: () => (request++ === 0 ? first.promise : second.promise),
        count: async () => 1
      }
    );

    const oldSearch = session.reset(query);
    const newSearch = session.reset({
      ...query,
      sort_field: 'title',
      sort_direction: 'ascending'
    });
    second.resolve({ items: [work('new')], next_cursor: null });
    await newSearch;
    first.resolve({ items: [work('old')], next_cursor: null });
    await oldSearch;

    expect(session.items.map((item) => item.id)).toEqual(['new']);
    expect(session.appliedQuery).toMatchObject({
      sort_field: 'title',
      sort_direction: 'ascending'
    });
  });

  it('runs a normal applied-query search when the restored cache is empty', async () => {
    const requests: GallerySearch[] = [];
    const session = new GallerySearchSession(
      {
        items: [],
        totalCount: 0,
        appliedQuery: query
      },
      {
        search: async (request) => {
          requests.push(structuredClone(request));
          return { items: [work('new')], next_cursor: null };
        },
        count: async () => 1
      }
    );

    await session.refreshFromAppliedQuery();

    expect(requests).toEqual([query]);
    expect(session.items.map((item) => item.id)).toEqual(['new']);
    expect(session.totalCount).toBe(1);
  });

  it('refreshes the applied query in the exact returned order', async () => {
    const original = [work('old-first'), work('old-second')];
    const session = new GallerySearchSession(
      { items: original, totalCount: 2, appliedQuery: query },
      {
        search: async () => ({
          items: [
            work('new-first'),
            { ...work('old-first'), title: 'updated title' }
          ],
          next_cursor: null
        }),
        count: async () => 3
      }
    );

    await session.refreshFromAppliedQuery();

    expect(session.items.map((item) => item.id)).toEqual([
      'new-first',
      'old-first'
    ]);
    expect(session.items[1]?.title).toBe('updated title');
    expect(session.totalCount).toBe(3);
  });

  it('keeps a restored snapshot unchanged when its refresh fails', async () => {
    const original = [work('kept'), work('stale')];
    const session = new GallerySearchSession(
      { items: original, totalCount: 8, appliedQuery: query },
      {
        search: async () => {
          throw new Error('unavailable');
        },
        count: async () => 1
      }
    );

    await session.refreshFromAppliedQuery();

    expect(session.items).toEqual(original);
    expect(session.totalCount).toBe(8);
  });

  it('releases a superseded background refresh when a manual search starts', async () => {
    const refreshPage = deferred<GallerySearchPage>();
    let searchRequest = 0;
    const session = new GallerySearchSession(
      { items: [work('cached')], totalCount: 1, appliedQuery: query },
      {
        search: async () => {
          searchRequest += 1;
          if (searchRequest === 1) return refreshPage.promise;
          return { items: [work('manual')], next_cursor: null };
        },
        count: async () => 1
      }
    );

    const refreshing = session.refreshFromAppliedQuery();
    expect(session.refreshing).toBe(true);
    await session.reset({
      ...query,
      sort_field: 'title',
      sort_direction: 'ascending'
    });

    expect(session.refreshing).toBe(false);
    expect(session.items.map((item) => item.id)).toEqual(['manual']);
    refreshPage.resolve({ items: [work('stale')], next_cursor: null });
    await refreshing;
    expect(session.refreshing).toBe(false);
    expect(session.items.map((item) => item.id)).toEqual(['manual']);
  });

  it('does not start pagination while a replacement refresh is running', async () => {
    const refreshPage = deferred<GallerySearchPage>();
    let searchRequests = 0;
    const session = new GallerySearchSession(
      {
        items: [work('cached')],
        cursor: {
          sort_field: 'pixiv_id',
          sort_direction: 'descending',
          key: { type: 'integer', value: 1 },
          work_id: 'cached'
        },
        totalCount: 2,
        appliedQuery: query
      },
      {
        search: () => {
          searchRequests += 1;
          return refreshPage.promise;
        },
        count: async () => 2
      }
    );

    const refreshing = session.refreshFromAppliedQuery();
    expect(await session.loadNext()).toBe(false);
    expect(searchRequests).toBe(1);
    refreshPage.resolve({ items: [work('refreshed')], next_cursor: null });
    await refreshing;
  });

  it('removes restored cards that no longer match during a loaded-item refresh', async () => {
    const original = [work('removed'), work('kept')];
    original[1]!.cover_width = 320;
    original[1]!.cover_height = 480;
    const session = new GallerySearchSession(
      { items: original, totalCount: 2, appliedQuery: query },
      {
        search: async () => ({
          items: [
            {
              ...work('kept'),
              title: 'updated',
              cover_width: 900,
              cover_height: 600
            }
          ],
          next_cursor: null
        }),
        count: async () => 1
      }
    );

    await session.refreshLoadedItems();

    expect(session.items.map((item) => item.id)).toEqual(['kept']);
    expect(session.items[0]).toMatchObject({
      title: 'updated',
      cover_width: 320,
      cover_height: 480
    });
    expect(session.totalCount).toBe(1);
  });

  it('invalidates a pending query selection when selection mode exits', async () => {
    const projection = deferred<{
      selected_count: number;
      selected_visible_work_ids: string[];
    }>();
    const session = new GallerySelectionSession({
      project: () => projection.promise,
      move: async () => 0
    });
    session.enter(query);

    const selecting = session.selectAll(['visible']);
    session.exit();
    projection.resolve({
      selected_count: 20,
      selected_visible_work_ids: ['visible']
    });
    await selecting;

    expect(session.mode).toBe(false);
    expect(session.busy).toBe(false);
    expect(session.count).toBe(0);
  });

  it('keeps the fixed query while projecting a different visible page', async () => {
    const projections = [
      {
        selected_count: 3,
        selected_visible_work_ids: ['first']
      },
      {
        selected_count: 3,
        selected_visible_work_ids: []
      }
    ];
    const expressions: unknown[] = [];
    const session = new GallerySelectionSession({
      project: async (expression) => {
        expressions.push(structuredClone(expression));
        return projections.shift()!;
      },
      move: async () => 0
    });
    session.enter(query);

    await session.selectAll(['first']);
    await session.refreshVisible(['outside']);

    expect(session.count).toBe(3);
    expect(session.idsFor([work('outside')])).toEqual(new Set());
    expect(expressions).toHaveLength(2);
    expect(expressions[1]).toEqual(expressions[0]);
    expect(session.snapshotExpression()).toEqual({
      search: query,
      base_selected: true,
      exception_work_ids: []
    });
  });

  it('reprojects the latest visible works after a pending selection commits', async () => {
    const initialProjection = deferred<{
      selected_count: number;
      selected_visible_work_ids: string[];
    }>();
    const refreshedProjection = deferred<{
      selected_count: number;
      selected_visible_work_ids: string[];
    }>();
    const requests: Array<{ expression: unknown; visibleWorkIds: string[] }> =
      [];
    const session = new GallerySelectionSession({
      project: (expression, visibleWorkIds) => {
        requests.push({
          expression: structuredClone(expression),
          visibleWorkIds: [...visibleWorkIds]
        });
        return requests.length === 1
          ? initialProjection.promise
          : refreshedProjection.promise;
      },
      move: async () => 0
    });
    session.enter(query);

    const selecting = session.selectAll(['old-visible']);
    const refreshing = session.refreshVisible(['new-visible']);
    initialProjection.resolve({
      selected_count: 9,
      selected_visible_work_ids: ['old-visible']
    });
    await vi.waitFor(() => expect(requests).toHaveLength(2));

    expect(requests).toEqual([
      {
        expression: {
          search: query,
          base_selected: true,
          exception_work_ids: []
        },
        visibleWorkIds: ['old-visible']
      },
      {
        expression: {
          search: query,
          base_selected: true,
          exception_work_ids: []
        },
        visibleWorkIds: ['new-visible']
      }
    ]);

    refreshedProjection.resolve({
      selected_count: 9,
      selected_visible_work_ids: ['new-visible']
    });
    await Promise.all([selecting, refreshing]);

    expect(session.count).toBe(9);
    expect(session.idsFor([work('new-visible')])).toEqual(
      new Set(['new-visible'])
    );
  });

  it('commits invert and single-work exceptions only after projection succeeds', async () => {
    let failNext = false;
    const session = new GallerySelectionSession({
      project: async (expression) => {
        if (failNext) throw new Error('unavailable');
        const excluded = expression.exception_work_ids?.includes('two');
        return {
          selected_count: excluded ? 2 : 4,
          selected_visible_work_ids: excluded ? ['one'] : ['one', 'two']
        };
      },
      move: async () => 0
    });
    session.enter(query);

    await session.selectAll(['one', 'two']);
    await session.setWork('two', false, ['one', 'two']);
    failNext = true;
    await session.selectAll(['one', 'two']);

    expect(session.count).toBe(2);
    expect(session.idsFor([work('one'), work('two')])).toEqual(
      new Set(['one'])
    );
    expect(session.snapshotExpression()).toEqual({
      search: query,
      base_selected: true,
      exception_work_ids: ['two']
    });
    expect(session.error).toBe('无法更新当前选择');
  });

  it('moves the complete expression with one collection command', async () => {
    const moved: unknown[] = [];
    const session = new GallerySelectionSession({
      project: async () => ({
        selected_count: 8,
        selected_visible_work_ids: ['one']
      }),
      move: async (expression, retentionDays) => {
        moved.push([structuredClone(expression), retentionDays]);
        return 8;
      }
    });
    session.enter(query);
    await session.selectAll(['one']);

    const result = await session.trash(30, false);

    expect(result).toEqual({ movedCount: 8 });
    expect(moved).toEqual([
      [
        {
          search: query,
          base_selected: true,
          exception_work_ids: []
        },
        30
      ]
    ]);
    expect(session.mode).toBe(false);
    expect(session.notice).toBe('8件作品已移入回收站');
  });

  it('keeps a fixed context query and reports deduplicated works', async () => {
    const expressions: GalleryContextSelectionExpression[] = [];
    const projections = [
      {
        selected_context_count: 2,
        selected_work_count: 5,
        selected_visible_context_ids: ['tag-one']
      },
      {
        selected_context_count: 2,
        selected_work_count: 5,
        selected_visible_context_ids: []
      }
    ];
    const session = new GalleryContextSelectionSession(() => 'tag', {
      project: async (expression) => {
        expressions.push(structuredClone(expression));
        return projections.shift()!;
      },
      move: async () => 0
    });
    session.enter('夜空');

    await session.selectAll(['tag-one']);
    await session.refreshVisible(['tag-outside']);

    expect(session.contextCount).toBe(2);
    expect(session.workCount).toBe(5);
    expect(session.idsFor([{ id: 'tag-outside' }])).toEqual(new Set());
    expect(expressions).toEqual([
      {
        kind: 'tag',
        query: '夜空',
        base_selected: true,
        exception_context_ids: []
      },
      {
        kind: 'tag',
        query: '夜空',
        base_selected: true,
        exception_context_ids: []
      }
    ]);
  });

  it('reprojects the latest directory items after a pending selection commits', async () => {
    const initialProjection = deferred<{
      selected_context_count: number;
      selected_work_count: number;
      selected_visible_context_ids: string[];
    }>();
    const refreshedProjection = deferred<{
      selected_context_count: number;
      selected_work_count: number;
      selected_visible_context_ids: string[];
    }>();
    const requests: Array<{
      expression: GalleryContextSelectionExpression;
      visibleContextIds: string[];
    }> = [];
    const session = new GalleryContextSelectionSession(() => 'tag', {
      project: (expression, visibleContextIds) => {
        requests.push({
          expression: structuredClone(expression),
          visibleContextIds: [...visibleContextIds]
        });
        return requests.length === 1
          ? initialProjection.promise
          : refreshedProjection.promise;
      },
      move: async () => 0
    });
    session.enter('夜空');

    const selecting = session.selectAll(['old-tag']);
    const refreshing = session.refreshVisible(['new-tag']);
    initialProjection.resolve({
      selected_context_count: 3,
      selected_work_count: 11,
      selected_visible_context_ids: ['old-tag']
    });
    await vi.waitFor(() => expect(requests).toHaveLength(2));

    expect(requests).toEqual([
      {
        expression: {
          kind: 'tag',
          query: '夜空',
          base_selected: true,
          exception_context_ids: []
        },
        visibleContextIds: ['old-tag']
      },
      {
        expression: {
          kind: 'tag',
          query: '夜空',
          base_selected: true,
          exception_context_ids: []
        },
        visibleContextIds: ['new-tag']
      }
    ]);

    refreshedProjection.resolve({
      selected_context_count: 3,
      selected_work_count: 11,
      selected_visible_context_ids: ['new-tag']
    });
    await Promise.all([selecting, refreshing]);

    expect(session.contextCount).toBe(3);
    expect(session.workCount).toBe(11);
    expect(session.idsFor([{ id: 'new-tag' }])).toEqual(new Set(['new-tag']));
  });

  it('commits context overrides only after projection succeeds', async () => {
    let failNext = false;
    const session = new GalleryContextSelectionSession(() => 'series', {
      project: async (expression) => {
        if (failNext) throw new Error('unavailable');
        const selected =
          expression.exception_context_ids?.includes('series-two');
        return {
          selected_context_count: selected ? 1 : 4,
          selected_work_count: selected ? 3 : 9,
          selected_visible_context_ids: selected ? ['series-one'] : []
        };
      },
      move: async () => 0
    });
    session.enter('画集');

    await session.selectAll(['series-one', 'series-two']);
    await session.setItem('series-two', false, ['series-one', 'series-two']);
    failNext = true;
    await session.selectAll(['series-one', 'series-two']);

    expect(session.contextCount).toBe(1);
    expect(session.workCount).toBe(3);
    expect(
      session.idsFor([{ id: 'series-one' }, { id: 'series-two' }])
    ).toEqual(new Set(['series-one']));
    expect(session.snapshotExpression()).toEqual({
      kind: 'series',
      query: '画集',
      base_selected: true,
      exception_context_ids: ['series-two']
    });
    expect(session.error).toBe('无法更新当前选择');
  });

  it('moves context collections with one command and uses the work count in feedback', async () => {
    const moved: unknown[] = [];
    const session = new GalleryContextSelectionSession(() => 'artist', {
      project: async () => ({
        selected_context_count: 2,
        selected_work_count: 7,
        selected_visible_context_ids: ['artist-one']
      }),
      move: async (expression, retentionDays) => {
        moved.push([structuredClone(expression), retentionDays]);
        return 7;
      }
    });
    session.enter('');
    await session.selectAll(['artist-one']);

    const result = await session.trash(30, false);

    expect(result).toEqual({ movedCount: 7 });
    expect(moved).toEqual([
      [
        {
          kind: 'artist',
          query: '',
          base_selected: true,
          exception_context_ids: []
        },
        30
      ]
    ]);
    expect(session.mode).toBe(false);
    expect(session.notice).toBe('7件作品已移入回收站');
  });

  it('keeps the latest gallery selection when projections finish out of order', async () => {
    const firstProjection = deferred<{
      selected_count: number;
      selected_visible_work_ids: string[];
    }>();
    const secondProjection = deferred<{
      selected_count: number;
      selected_visible_work_ids: string[];
    }>();
    let request = 0;
    const session = new GallerySelectionSession({
      project: () =>
        request++ === 0 ? firstProjection.promise : secondProjection.promise,
      move: async () => 0
    });
    session.enter(query);

    const selectingFirst = session.setWork('one', true, ['one', 'two']);
    expect(session.idsFor([work('one'), work('two')])).toEqual(
      new Set(['one'])
    );
    const selectingSecond = session.setWork('two', true, ['one', 'two']);
    expect(request).toBe(2);
    expect(session.idsFor([work('one'), work('two')])).toEqual(
      new Set(['one', 'two'])
    );

    secondProjection.resolve({
      selected_count: 2,
      selected_visible_work_ids: ['one', 'two']
    });
    await selectingSecond;
    firstProjection.resolve({
      selected_count: 1,
      selected_visible_work_ids: ['one']
    });
    await selectingFirst;

    expect(session.count).toBe(2);
    expect(session.idsFor([work('one'), work('two')])).toEqual(
      new Set(['one', 'two'])
    );
    expect(session.snapshotExpression().exception_work_ids).toEqual([
      'one',
      'two'
    ]);
  });

  it('restores the local gallery intent that preceded a failed latest projection', async () => {
    const firstProjection = deferred<{
      selected_count: number;
      selected_visible_work_ids: string[];
    }>();
    let request = 0;
    const session = new GallerySelectionSession({
      project: async () => {
        if (request++ === 0) return firstProjection.promise;
        throw new Error('unavailable');
      },
      move: async () => 0
    });
    session.enter(query);

    const selectingFirst = session.setWork('one', true, ['one', 'two']);
    await session.setWork('two', true, ['one', 'two']);

    expect(session.count).toBe(1);
    expect(session.idsFor([work('one'), work('two')])).toEqual(
      new Set(['one'])
    );
    expect(session.snapshotExpression().exception_work_ids).toEqual(['one']);
    expect(session.error).toBe('无法更新当前选择');

    firstProjection.resolve({
      selected_count: 1,
      selected_visible_work_ids: ['one']
    });
    await selectingFirst;
  });

  it('waits for the latest gallery selection before moving the collection', async () => {
    const projection = deferred<{
      selected_count: number;
      selected_visible_work_ids: string[];
    }>();
    const moved: unknown[] = [];
    const session = new GallerySelectionSession({
      project: () => projection.promise,
      move: async (expression) => {
        moved.push(structuredClone(expression));
        return 1;
      }
    });
    session.enter(query);

    const selecting = session.setWork('one', true, ['one']);
    const moving = session.trash(30, false);
    expect(moved).toEqual([]);

    projection.resolve({
      selected_count: 1,
      selected_visible_work_ids: ['one']
    });
    await selecting;
    await moving;

    expect(moved).toEqual([
      {
        search: query,
        base_selected: false,
        exception_work_ids: ['one']
      }
    ]);
  });

  it('keeps the latest context selection when projections finish out of order', async () => {
    const firstProjection = deferred<{
      selected_context_count: number;
      selected_work_count: number;
      selected_visible_context_ids: string[];
    }>();
    const secondProjection = deferred<{
      selected_context_count: number;
      selected_work_count: number;
      selected_visible_context_ids: string[];
    }>();
    let request = 0;
    const session = new GalleryContextSelectionSession(() => 'artist', {
      project: () =>
        request++ === 0 ? firstProjection.promise : secondProjection.promise,
      move: async () => 0
    });
    session.enter('');

    const selectingFirst = session.setItem('one', true, ['one', 'two']);
    const selectingSecond = session.setItem('two', true, ['one', 'two']);
    expect(request).toBe(2);
    expect(session.idsFor([{ id: 'one' }, { id: 'two' }])).toEqual(
      new Set(['one', 'two'])
    );

    secondProjection.resolve({
      selected_context_count: 2,
      selected_work_count: 5,
      selected_visible_context_ids: ['one', 'two']
    });
    await selectingSecond;
    firstProjection.resolve({
      selected_context_count: 1,
      selected_work_count: 2,
      selected_visible_context_ids: ['one']
    });
    await selectingFirst;

    expect(session.contextCount).toBe(2);
    expect(session.workCount).toBe(5);
    expect(session.idsFor([{ id: 'one' }, { id: 'two' }])).toEqual(
      new Set(['one', 'two'])
    );
  });

  it('restores the local context intent that preceded a failed latest projection', async () => {
    const firstProjection = deferred<{
      selected_context_count: number;
      selected_work_count: number;
      selected_visible_context_ids: string[];
    }>();
    let request = 0;
    const session = new GalleryContextSelectionSession(() => 'artist', {
      project: async () => {
        if (request++ === 0) return firstProjection.promise;
        throw new Error('unavailable');
      },
      move: async () => 0
    });
    session.enter('');

    const selectingFirst = session.setItem('one', true, ['one', 'two']);
    await session.setItem('two', true, ['one', 'two']);

    expect(session.contextCount).toBe(0);
    expect(session.idsFor([{ id: 'one' }, { id: 'two' }])).toEqual(
      new Set(['one'])
    );
    expect(session.snapshotExpression().exception_context_ids).toEqual(['one']);
    expect(session.error).toBe('无法更新当前选择');

    firstProjection.resolve({
      selected_context_count: 1,
      selected_work_count: 2,
      selected_visible_context_ids: ['one']
    });
    await selectingFirst;
  });
});

function work(id: string): GalleryWork {
  return {
    id,
    pixiv_work_id: 1,
    title: id,
    description: null,
    artist_id: 'artist',
    pixiv_artist_id: 2,
    artist_name: 'artist',
    series_id: null,
    series_title: null,
    work_kind: 'illustration',
    age_rating: 'all_age',
    ai_generated: false,
    page_count: 1,
    collection_state: 'collected',
    source_state: 'present',
    bookmarked_by_current_account: false,
    bookmark_id: null,
    bookmark_count: null,
    view_count: null,
    like_count: null,
    comment_count: null,
    pixiv_published_at: null,
    pixiv_updated_at: null,
    local_updated_at: '2026-08-03T00:00:00Z',
    cover_available: true,
    cover_url: '/cover',
    cover_width: 100,
    cover_height: 100,
    media_kind: 'source_image',
    tags: []
  };
}

function deferred<T>(): {
  promise: Promise<T>;
  resolve: (value: T) => void;
} {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((next) => {
    resolve = next;
  });
  return { promise, resolve };
}
