import { expect, test } from '@playwright/test';

import {
  fulfillJson,
  mockApi,
  mockEffectiveSettings,
  PIXIV_ACCOUNT_ID
} from './support';

const WORK_ID = '0198f64c-42a2-7374-bace-9f1c3b317fa1';
const ARTIST_ID = '0198f64c-42a2-7374-bace-9f1c3b317fa2';
const OTHER_WORK_ID = '0198f64c-42a2-7374-bace-9f1c3b317fa7';
const NEW_WORK_ID = '0198f64c-42a2-7374-bace-9f1c3b317fa8';

test('gallery serializes structured search and keeps Ugoira badges visible', async ({
  page
}) => {
  await mockApi(page);
  let searchBody: unknown;
  await page.route('**/api/gallery/search', async (route) => {
    searchBody = route.request().postDataJSON();
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({ items: [galleryWork()], next_cursor: null })
    });
  });
  await page.route('**/api/gallery/count', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({ count: 194 })
    })
  );
  await page.route('**/api/derivatives/*', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'image/svg+xml',
      body: '<svg xmlns="http://www.w3.org/2000/svg" width="640" height="900"><rect width="640" height="900" fill="#2f7fd8"/></svg>'
    })
  );

  await page.goto('/gallery');
  await expect(page.getByRole('heading', { name: '图库' })).toBeVisible();
  await expect(page.getByText('194件作品，已显示1件')).toBeVisible();
  await expect(page.getByText('动图', { exact: true })).toBeVisible();
  await expect(page.getByText('R-18', { exact: true })).toHaveCount(0);

  await page.getByPlaceholder('搜索标题、作者、标签或Pixiv ID').fill('夜空');
  await page.getByRole('button', { name: '搜索', exact: true }).click();

  await expect
    .poll(() => searchBody)
    .toMatchObject({
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
      ]
    });

  await page.getByRole('button', { name: '筛选条件' }).click();
  await expect(
    page
      .getByRole('group', { name: '作品类型' })
      .getByRole('checkbox', { name: '插画' })
      .locator('..')
  ).toHaveCSS('border-radius', '999px');
  await page.getByLabel('标签', { exact: true }).fill('風景, 夜空');
  await page.getByLabel('标签匹配').click();
  await page.getByRole('option', { name: '全部标签' }).click();
  await page.getByRole('button', { name: '应用筛选' }).click();
  await expect
    .poll(
      () =>
        (
          searchBody as {
            groups: Array<{ filters: unknown[] }>;
          }
        ).groups[0].filters
    )
    .toEqual(
      expect.arrayContaining([
        {
          type: 'tags',
          operator: 'all',
          names: ['風景', '夜空'],
          scope: 'original_and_translation'
        }
      ])
    );
});

test('non-all-age masking uses the shared placeholder in gallery and context cards', async ({
  page
}) => {
  await mockApi(page);
  await page.unroute('**/api/system/settings');
  await page.route('**/api/system/settings', (route) =>
    fulfillJson(route, 200, {
      value: mockEffectiveSettings({ mask_non_all_age_thumbnails: true })
    })
  );
  await page.route('**/api/gallery/search', (route) =>
    fulfillJson(route, 200, {
      items: [{ ...galleryWork(), age_rating: 'r18' }],
      next_cursor: null
    })
  );
  await page.route('**/api/gallery/artists?*', (route) =>
    fulfillJson(route, 200, {
      items: [
        {
          id: ARTIST_ID,
          pixiv_artist_id: 2001,
          name: '受限封面作者',
          account_name: null,
          work_count: 1,
          cover_url: '/api/derivatives/restricted-context',
          cover_width: 640,
          cover_height: 900,
          cover_age_rating: 'r18'
        }
      ],
      total: 1,
      next_cursor: null
    })
  );
  let derivativeRequests = 0;
  await page.route('**/api/derivatives/*', async (route) => {
    derivativeRequests += 1;
    await route.fulfill({ status: 200, body: 'unexpected image request' });
  });

  await page.goto('/gallery');
  await expect(page.getByText('缩略图已遮挡')).toBeVisible();
  expect(derivativeRequests).toBe(0);

  await page.goto('/gallery/artists');
  await expect(page.getByText('缩略图已遮挡')).toBeVisible();
  expect(derivativeRequests).toBe(0);
});

test('gallery pauses automatic pagination after an error and retries on demand', async ({
  page
}) => {
  await mockApi(page);
  let searchCalls = 0;
  let paginationFailed = false;
  page.on('request', (request) => {
    if (new URL(request.url()).pathname === '/api/gallery/search') {
      searchCalls += 1;
    }
  });
  await page.route('**/api/gallery/search', async (route) => {
    const body = route.request().postDataJSON() as { cursor?: unknown };
    if (!body.cursor) {
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({
          items: [galleryWork()],
          next_cursor: {
            key: { type: 'integer', value: 1001 },
            work_id: WORK_ID
          }
        })
      });
      return;
    }
    if (!paginationFailed) {
      paginationFailed = true;
      await route.fulfill({
        status: 503,
        contentType: 'application/json',
        body: JSON.stringify({ error: 'temporary_failure' })
      });
      return;
    }
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        items: [
          {
            ...galleryWork(),
            id: OTHER_WORK_ID,
            pixiv_work_id: 1002,
            title: '雨后的街道'
          }
        ],
        next_cursor: null
      })
    });
  });

  await page.goto('/gallery');
  await expect(page.getByText('夏夜の星', { exact: true })).toBeVisible();
  await expect(page.getByText('后续作品暂时无法读取')).toBeVisible();
  await page.evaluate(
    () =>
      new Promise<void>((resolve) => {
        window.dispatchEvent(new Event('scroll'));
        requestAnimationFrame(() => requestAnimationFrame(() => resolve()));
      })
  );
  expect(searchCalls).toBe(2);

  await page
    .getByRole('alert')
    .getByRole('button', { name: '重新加载' })
    .click();
  await expect(page.getByText('雨后的街道', { exact: true })).toBeVisible();
  expect(searchCalls).toBe(3);
});

test('gallery keeps the query fixed while selecting every matching work', async ({
  page
}) => {
  await mockApi(page);
  let selectionRequest: unknown;
  let releaseSelectionProjection: () => void;
  const selectionProjectionReady = new Promise<void>((resolve) => {
    releaseSelectionProjection = resolve;
  });
  await page.route('**/api/gallery/search', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({ items: [galleryWork()], next_cursor: null })
    })
  );
  await page.route('**/api/gallery/count', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({ count: 1 })
    })
  );
  await page.route('**/api/gallery/selection', async (route) => {
    selectionRequest = route.request().postDataJSON();
    await selectionProjectionReady;
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        selected_count: 12,
        selected_visible_work_ids: [WORK_ID]
      })
    });
  });

  await page.goto('/gallery');
  await expect(page.getByText('夏夜の星', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: '多选' }).click();
  await page.getByRole('button', { name: '全选', exact: true }).click();

  await expect(
    page.getByRole('button', { name: '搜索', exact: true })
  ).toHaveCount(0);
  await expect(page.getByRole('button', { name: '退出多选' })).toBeEnabled();
  releaseSelectionProjection!();
  await expect(page.getByText('12件已选择')).toBeVisible();
  expect(selectionRequest).toEqual({
    expression: {
      search: {
        group_mode: 'all',
        groups: [],
        sort_field: 'pixiv_id',
        sort_direction: 'descending',
        limit: 60
      },
      base_selected: true,
      exception_work_ids: []
    },
    visible_work_ids: [WORK_ID]
  });
});

test('gallery reprojects a query selection when an in-flight refresh replaces visible works', async ({
  page
}) => {
  await mockApi(page);
  await page.unroute('**/api/events');
  let publishWorkChange!: () => void;
  const workChange = new Promise<void>((resolve) => {
    publishWorkChange = resolve;
  });
  let eventConnections = 0;
  await page.route('**/api/events', async (route) => {
    eventConnections += 1;
    if (eventConnections === 1) await workChange;
    await route.fulfill({
      status: 200,
      headers: { 'Content-Type': 'text/event-stream' },
      body:
        eventConnections === 1
          ? 'event: app_event\ndata: {"resource":"work","resource_id":"work-1"}\n\n'
          : 'retry: 60000\n\n'
    });
  });

  let refreshStarted!: () => void;
  const backgroundRefreshStarted = new Promise<void>((resolve) => {
    refreshStarted = resolve;
  });
  let releaseRefresh!: () => void;
  const backgroundRefreshReleased = new Promise<void>((resolve) => {
    releaseRefresh = resolve;
  });
  let searchRequests = 0;
  await page.route('**/api/gallery/search', async (route) => {
    searchRequests += 1;
    if (searchRequests > 1) {
      refreshStarted();
      await backgroundRefreshReleased;
    }
    const items =
      searchRequests === 1
        ? [
            galleryWork(),
            {
              ...galleryWork(),
              id: OTHER_WORK_ID,
              pixiv_work_id: 1002,
              title: '刷新后移除'
            }
          ]
        : [
            galleryWork(),
            {
              ...galleryWork(),
              id: NEW_WORK_ID,
              pixiv_work_id: 1003,
              title: '刷新后新增'
            }
          ];
    await fulfillJson(route, 200, { items, next_cursor: null });
  });
  await page.unroute('**/api/gallery/count');
  await page.route('**/api/gallery/count', (route) =>
    fulfillJson(route, 200, { count: 12 })
  );
  const projectionRequests: string[][] = [];
  await page.route('**/api/gallery/selection', async (route) => {
    const request = route.request().postDataJSON() as {
      visible_work_ids: string[];
    };
    projectionRequests.push([...request.visible_work_ids]);
    await fulfillJson(route, 200, {
      selected_count: 12,
      selected_visible_work_ids: request.visible_work_ids
    });
  });

  await page.goto('/gallery');
  await expect(page.getByText('刷新后移除', { exact: true })).toBeVisible();
  publishWorkChange();
  await backgroundRefreshStarted;

  await page.getByRole('button', { name: '多选' }).click();
  await page.getByRole('button', { name: '全选', exact: true }).click();
  await expect(page.getByText('12件已选择')).toBeVisible();
  releaseRefresh();

  await expect(page.getByText('刷新后新增', { exact: true })).toBeVisible();
  await expect(page.getByText('刷新后移除', { exact: true })).toHaveCount(0);
  await expect(
    page.getByRole('checkbox', { name: '选择刷新后新增' })
  ).toBeChecked();
  await expect(page.getByText('12件已选择')).toBeVisible();
  expect(projectionRequests).toEqual([
    [WORK_ID, OTHER_WORK_ID],
    [WORK_ID, NEW_WORK_ID]
  ]);
});

test('waterfall responds to viewport width and renders the final cards at its bottom', async ({
  page
}) => {
  await page.setViewportSize({ width: 1280, height: 720 });
  await mockApi(page);
  await page.route('**/api/gallery/search', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        items: Array.from({ length: 40 }, (_, index) => ({
          ...galleryWork(),
          id: `0198f64c-42a2-7374-bace-${(index + 1)
            .toString(16)
            .padStart(12, '0')}`,
          pixiv_work_id: 1001 + index,
          title: `作品${index + 1}`,
          work_kind: 'illustration',
          media_kind: 'image'
        })),
        next_cursor: null
      })
    })
  );
  await page.route('**/api/derivatives/*', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'image/svg+xml',
      body: '<svg xmlns="http://www.w3.org/2000/svg" width="640" height="900"><rect width="640" height="900" fill="#2f7fd8"/></svg>'
    })
  );

  await page.goto('/gallery');
  const cards = page.locator('.gallery-card');
  await expect(cards.first()).toBeVisible();

  const wideColumns = await page
    .locator('.card-slot')
    .evaluateAll(
      (elements) =>
        new Set(elements.map((element) => getComputedStyle(element).left)).size
    );
  await page.setViewportSize({ width: 900, height: 720 });
  await expect
    .poll(() =>
      page
        .locator('.card-slot')
        .evaluateAll(
          (elements) =>
            new Set(elements.map((element) => getComputedStyle(element).left))
              .size
        )
    )
    .toBeLessThan(wideColumns);

  const minimumGap = await cards.evaluateAll((elements) => {
    const columns = new Map<number, Array<{ top: number; bottom: number }>>();
    for (const element of elements) {
      const rect = element.getBoundingClientRect();
      const left = Math.round(rect.left);
      const column = columns.get(left) ?? [];
      column.push({ top: rect.top, bottom: rect.bottom });
      columns.set(left, column);
    }

    const gaps = [...columns.values()].flatMap((column) => {
      column.sort((left, right) => left.top - right.top);
      return column
        .slice(1)
        .map((card, index) => card.top - column[index].bottom);
    });
    return Math.min(...gaps);
  });

  expect(minimumGap).toBeGreaterThanOrEqual(0);

  await page.evaluate(() => window.scrollTo(0, document.body.scrollHeight));
  await expect(page.getByText('作品40', { exact: true })).toBeVisible();
  const bottomGap = await page.evaluate(() => {
    const canvas = document.querySelector('.waterfall-canvas');
    const slots = [...document.querySelectorAll('.card-slot')];
    if (!canvas || slots.length === 0) throw new Error('waterfall is missing');
    const lastCardBottom = Math.max(
      ...slots.map((slot) => slot.getBoundingClientRect().bottom)
    );
    return canvas.getBoundingClientRect().bottom - lastCardBottom;
  });
  expect(bottomGap).toBeLessThanOrEqual(1);
});

test('gallery multi-select moves complete works to trash', async ({ page }) => {
  await mockApi(page);
  await page.route('**/api/derivatives/*', (route) =>
    route.fulfill({ status: 404, contentType: 'application/json', body: '{}' })
  );
  await page.route('**/api/gallery/search', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        items: [
          galleryWork(),
          {
            ...galleryWork(),
            id: OTHER_WORK_ID,
            pixiv_work_id: 1002,
            title: '雨后的街道'
          }
        ],
        next_cursor: null
      })
    })
  );
  await page.route('**/api/gallery/selection', async (route) => {
    const request = route.request().postDataJSON() as {
      expression: {
        base_selected?: boolean;
        exception_work_ids?: string[];
      };
      visible_work_ids: string[];
    };
    const exceptions = new Set(request.expression.exception_work_ids ?? []);
    const baseSelected = request.expression.base_selected ?? false;
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        selected_count: baseSelected ? 2 - exceptions.size : exceptions.size,
        selected_visible_work_ids: request.visible_work_ids.filter(
          (workId) => baseSelected !== exceptions.has(workId)
        )
      })
    });
  });
  let trashRequest: unknown;
  await page.route('**/api/gallery/trash', async (route) => {
    trashRequest = route.request().postDataJSON();
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({ moved_count: 2 })
    });
  });
  await page.goto('/gallery');
  await expect(page.getByText('缩略图未加载').first()).toBeVisible();
  await page.getByRole('button', { name: '多选' }).click();
  await page.getByRole('checkbox', { name: '选择夏夜の星' }).check();
  await page.getByRole('checkbox', { name: '选择雨后的街道' }).check();
  const moveToTrash = page.getByRole('button', { name: '移入回收站' });
  await moveToTrash.click();
  let confirmation = page.getByRole('dialog', { name: '移入回收站' });
  await expect(confirmation).toContainText('2件作品');
  await expect(
    confirmation.getByRole('button', { name: '取消' })
  ).toBeFocused();
  await page.keyboard.press('Shift+Tab');
  await expect(
    confirmation.getByRole('button', { name: '移入回收站' })
  ).toBeFocused();
  await page.keyboard.press('Escape');
  await expect(confirmation).toBeHidden();
  await expect(moveToTrash).toBeFocused();

  await moveToTrash.click();
  confirmation = page.getByRole('dialog', { name: '移入回收站' });
  await confirmation.getByRole('button', { name: '移入回收站' }).click();

  await expect(page.getByText('2件作品已移入回收站')).toBeVisible();
  expect(trashRequest).toEqual({
    expression: {
      search: {
        group_mode: 'all',
        groups: [],
        sort_field: 'pixiv_id',
        sort_direction: 'descending',
        limit: 60
      },
      base_selected: false,
      exception_work_ids: [WORK_ID, OTHER_WORK_ID]
    },
    retention_days: 30
  });
});

test('gallery selection mode removes card navigation while preserving selection controls', async ({
  page
}) => {
  await mockApi(page);
  await page.route('**/api/gallery/search', (route) =>
    fulfillJson(route, 200, {
      items: [galleryWork()],
      next_cursor: null
    })
  );
  await page.route('**/api/gallery/selection', async (route) => {
    const request = route.request().postDataJSON() as {
      visible_work_ids: string[];
    };
    await fulfillJson(route, 200, {
      selected_count: 1,
      selected_visible_work_ids: request.visible_work_ids
    });
  });

  await page.goto('/gallery');
  const card = page.locator('.gallery-card');
  await expect(card.getByRole('link')).toHaveCount(4);

  await page.getByRole('button', { name: '多选' }).click();

  await expect(card.getByRole('link')).toHaveCount(0);
  await expect(card.getByText('Sample Artist', { exact: true })).toBeVisible();
  await expect(card.getByText('#夜空', { exact: true })).toBeVisible();
  await card.getByRole('button', { name: '切换选择夏夜の星' }).click();
  await expect(
    page.getByRole('checkbox', { name: '选择夏夜の星' })
  ).toBeChecked();
  await expect(page).toHaveURL('/gallery');
});

test('gallery selection keeps thumbnail nodes and card geometry stable without opacity flashing', async ({
  page
}) => {
  await mockApi(page);
  let releaseSelection!: () => void;
  const selectionReleased = new Promise<void>((resolve) => {
    releaseSelection = resolve;
  });
  await page.route('**/api/gallery/search', (route) =>
    fulfillJson(route, 200, { items: [galleryWork()], next_cursor: null })
  );
  await page.route('**/api/gallery/selection', async (route) => {
    await selectionReleased;
    const request = route.request().postDataJSON() as {
      visible_work_ids: string[];
    };
    await fulfillJson(route, 200, {
      selected_count: 1,
      selected_visible_work_ids: request.visible_work_ids
    });
  });

  await page.goto('/gallery');
  const card = page.locator('.gallery-card');
  const geometryBefore = await card.boundingBox();
  await card.locator('.work-thumbnail').evaluate((thumbnail) => {
    (window as Window & { __galleryThumbnail?: Element }).__galleryThumbnail =
      thumbnail;
  });
  await page.getByRole('button', { name: '多选' }).click();
  const geometryDuring = await card.boundingBox();
  expect(geometryDuring).toEqual(geometryBefore);
  expect(
    await card
      .locator('.work-thumbnail')
      .evaluate(
        (thumbnail) =>
          (window as Window & { __galleryThumbnail?: Element })
            .__galleryThumbnail === thumbnail
      )
  ).toBe(true);

  const selectAll = page.getByRole('button', { name: '全选', exact: true });
  const opacityBefore = await selectAll.evaluate(
    (button) => getComputedStyle(button).opacity
  );
  await selectAll.click();
  await expect(selectAll).toBeEnabled();
  await expect(
    page.getByRole('checkbox', { name: '选择夏夜の星' })
  ).toBeChecked();
  expect(
    await selectAll.evaluate((button) => getComputedStyle(button).opacity)
  ).toBe(opacityBefore);
  expect(
    await card.evaluate((element) => getComputedStyle(element).opacity)
  ).toBe('1');
  releaseSelection();
  await expect(page.getByText('1件已选择')).toBeVisible();
});

test('gallery restores loaded cards and scroll position after opening a work', async ({
  page
}) => {
  await page.setViewportSize({ width: 1000, height: 600 });
  await mockApi(page);
  const searches: Array<{ url: string; body: unknown }> = [];
  await page.route('**/api/gallery/search', (route) => {
    const body = route.request().postDataJSON();
    const refreshingRestoredItems =
      Array.isArray(body.restrict_work_ids) &&
      body.restrict_work_ids.length > 0;
    if (body.limit !== 1) {
      searches.push({ url: route.request().url(), body });
    }
    return route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        items: Array.from({ length: 36 }, (_, index) => ({
          ...galleryWork(),
          id: `0198f64c-42a2-7374-bace-${(index + 1)
            .toString(16)
            .padStart(12, '0')}`,
          pixiv_work_id: 1001 + index,
          title: `作品${index + 1}`,
          bookmarked_by_current_account: refreshingRestoredItems
        })),
        next_cursor: null
      })
    });
  });
  await page.route('**/api/derivatives/*', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'image/svg+xml',
      body: '<svg xmlns="http://www.w3.org/2000/svg" width="640" height="900"><rect width="640" height="900" fill="#2f7fd8"/></svg>'
    })
  );
  await page.route('**/api/gallery/works/*', (route) =>
    route.fulfill({ status: 404, contentType: 'application/json', body: '{}' })
  );

  await page.goto('/gallery');
  await expect
    .poll(() => page.evaluate(() => document.body.scrollHeight))
    .toBeGreaterThan(2000);
  await page.evaluate(() => window.scrollTo(0, document.body.scrollHeight));
  await expect(page.getByText('作品36', { exact: true })).toBeVisible();
  const before = await page.evaluate(() => window.scrollY);
  const searchesBeforeOpen = searches.length;
  await page.getByText('作品36', { exact: true }).click();
  await expect(page).toHaveURL(/\/gallery\/works\/1036$/);
  await page.goBack();

  await expect(page.getByText('作品36', { exact: true })).toBeVisible();
  await expect.poll(() => page.evaluate(() => window.scrollY)).toBe(before);
  await expect(page.getByLabel('已收藏').first()).toBeVisible();
  expect(
    await page.evaluate(() =>
      sessionStorage.getItem('pixivarchive.gallery-return')
    )
  ).toBeNull();
  expect(searches.slice(searchesBeforeOpen)).toHaveLength(1);
});

test('work detail navigation returns to the specific gallery context that opened it', async ({
  page
}) => {
  await page.setViewportSize({ width: 1000, height: 600 });
  await mockApi(page);
  await page.route('**/api/gallery/artists/2001', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        id: ARTIST_ID,
        pixiv_artist_id: 2001,
        name: 'Sample Artist',
        account_name: null,
        work_count: 1,
        cover_url: null,
        cover_width: null,
        cover_height: null
      })
    })
  );
  await page.route('**/api/gallery/search', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        items: Array.from({ length: 36 }, (_, index) => ({
          ...galleryWork(),
          id: `0198f64c-42a2-7374-bace-${(index + 1500)
            .toString(16)
            .padStart(12, '0')}`,
          pixiv_work_id: 1001 + index,
          title: `作者作品${index + 1}`
        })),
        next_cursor: null
      })
    })
  );
  await page.route('**/api/gallery/works/*', (route) =>
    route.fulfill({ status: 404, contentType: 'application/json', body: '{}' })
  );

  await page.goto('/gallery/artists/2001');
  await expect
    .poll(() => page.evaluate(() => document.body.scrollHeight))
    .toBeGreaterThan(2000);
  await page.evaluate(() => window.scrollTo(0, document.body.scrollHeight));
  await expect(page.getByText('作者作品36', { exact: true })).toBeVisible();
  const scrollBeforeOpen = await page.evaluate(() => window.scrollY);
  await page.getByText('作者作品36', { exact: true }).click();
  await expect(page).toHaveURL('/gallery/works/1036');

  const returnLink = page
    .getByRole('navigation', { name: '二级导航' })
    .getByRole('link', { name: '返回' });
  await expect(returnLink).toHaveAttribute('href', '/gallery/artists/2001');
  await returnLink.click();

  await expect(page).toHaveURL('/gallery/artists/2001');
  await expect
    .poll(() =>
      page.evaluate(
        (expectedScrollY) => Math.abs(window.scrollY - expectedScrollY),
        scrollBeforeOpen
      )
    )
    .toBeLessThan(2);
  await expect(page.getByText('作者作品36', { exact: true })).toBeVisible();
});

test('context indexes return to the card that opened a result', async ({
  page
}) => {
  await page.setViewportSize({ width: 1000, height: 600 });
  await mockApi(page);
  const artists = Array.from({ length: 160 }, (_, index) => ({
    id: `0198f64c-42a2-7374-bace-${(index + 100)
      .toString(16)
      .padStart(12, '0')}`,
    pixiv_artist_id: 2000 + index,
    name: `作者${index + 1}`,
    account_name: null,
    work_count: index + 1,
    cover_url: null,
    cover_width: null,
    cover_height: null
  }));
  const tags = Array.from({ length: 160 }, (_, index) => ({
    tag: {
      id: `0198f64c-42a2-7374-bace-${(index + 300)
        .toString(16)
        .padStart(12, '0')}`,
      original: `标签${index + 1}`,
      translation: null
    },
    work_count: index + 1,
    cover_url: null,
    cover_width: null,
    cover_height: null
  }));
  const series = Array.from({ length: 400 }, (_, index) => ({
    id: `0198f64c-42a2-7374-bace-${(index + 600)
      .toString(16)
      .padStart(12, '0')}`,
    pixiv_series_id: 5000 + index,
    pixiv_artist_id: 2000,
    title: `系列${index + 1}`,
    work_count: index + 1,
    cover_url: null,
    cover_width: null,
    cover_height: null
  }));
  const contextPage = <T>(url: string, items: T[]) => {
    const params = new URL(url).searchParams;
    const cursor = Number(params.get('cursor') ?? 0);
    const limit = Number(params.get('limit') ?? 48);
    const pageItems = items.slice(cursor, cursor + limit);
    const nextCursor = cursor + pageItems.length;
    return {
      items: pageItems,
      total: items.length,
      next_cursor: nextCursor < items.length ? String(nextCursor) : null
    };
  };
  await page.route('**/api/gallery/artists?*', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify(contextPage(route.request().url(), artists))
    })
  );
  await page.route('**/api/gallery/artists/*', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify(artists.at(-1))
    })
  );
  await page.route('**/api/gallery/tags?*', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify(contextPage(route.request().url(), tags))
    })
  );
  await page.route('**/api/gallery/tags/*', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify(tags.at(-1))
    })
  );
  await page.route('**/api/gallery/series?*', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify(contextPage(route.request().url(), series))
    })
  );
  await page.route('**/api/gallery/search', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({ items: [], next_cursor: null })
    })
  );

  for (const context of [
    {
      route: '/gallery/artists',
      card: 'a[href^="/gallery/artists/"]',
      index: 130,
      count: 160,
      status: '160位作者',
      expectedHref: '/gallery/artists/2130'
    },
    {
      route: '/gallery/tags',
      card: 'a[href^="/gallery/tags/"]',
      index: 130,
      count: 160,
      status: '160个标签',
      expectedHref: '/gallery/tags/%E6%A0%87%E7%AD%BE131'
    },
    {
      route: '/gallery/series',
      card: 'a[href^="/gallery/series/"]',
      index: 130,
      count: 400,
      status: '400个系列',
      expectedHref: '/gallery/series/5130'
    }
  ]) {
    await page.goto(context.route);
    const cards = page.locator(context.card);
    await expect(
      page.getByText(context.status, { exact: false })
    ).toBeVisible();
    await expect
      .poll(
        async () => {
          await page.evaluate(() =>
            window.scrollTo(0, document.body.scrollHeight)
          );
          return cards.count();
        },
        { timeout: 15_000 }
      )
      .toBe(context.count);
    const card = cards.nth(context.index);
    await expect(card).toHaveAttribute('href', context.expectedHref);
    await card.scrollIntoViewIfNeeded();
    const before = await page.evaluate(() => window.scrollY);
    expect(before).toBeGreaterThan(0);
    // Playwright's actionability scroll would change the position under test.
    await card.evaluate((element: HTMLAnchorElement) => element.click());
    await expect(page).not.toHaveURL(context.route);
    await page.goBack();
    await expect(card).toBeVisible();
    await expect
      .poll(() =>
        page.evaluate((value) => Math.abs(window.scrollY - value), before)
      )
      .toBeLessThan(4);
  }
});

test('context directory retries the failed search or page that the user requested', async ({
  page
}) => {
  await mockApi(page);
  let resetAttempts = 0;
  let nextAttempts = 0;
  const tags = Array.from({ length: 49 }, (_, index) => ({
    tag: {
      id: `0198f64c-42a2-7374-bace-${(index + 900)
        .toString(16)
        .padStart(12, '0')}`,
      original: `重试标签${index + 1}`,
      translation: null
    },
    work_count: index + 1,
    cover_url: null,
    cover_width: null,
    cover_height: null
  }));
  await page.route('**/api/gallery/tags?*', async (route) => {
    const params = new URL(route.request().url()).searchParams;
    const query = params.get('q') ?? '';
    const cursor = Number(params.get('cursor') ?? 0);
    if (!query) {
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({ items: [], total: 0, next_cursor: null })
      });
      return;
    }
    if (cursor === 0 && resetAttempts++ === 0) {
      await route.fulfill({ status: 503 });
      return;
    }
    if (cursor === 48 && nextAttempts++ === 0) {
      await route.fulfill({ status: 503 });
      return;
    }
    const items = tags.slice(cursor, cursor + 48);
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        items,
        total: tags.length,
        next_cursor:
          cursor + items.length < tags.length
            ? String(cursor + items.length)
            : null
      })
    });
  });

  await page.goto('/gallery/tags');
  await page.getByPlaceholder('搜索标签').fill('重试');
  await page.getByRole('button', { name: '搜索', exact: true }).click();
  await expect(page.getByText('标签列表暂时无法读取')).toBeVisible();
  await page.getByRole('button', { name: '重新加载' }).click();
  await expect(page.getByText('重试标签1', { exact: true })).toBeVisible();
  expect(resetAttempts).toBe(2);

  await page.evaluate(() => window.scrollTo(0, document.body.scrollHeight));
  await expect(page.getByText('标签列表暂时无法读取')).toBeVisible();
  await page.getByRole('button', { name: '重新加载' }).click();
  await expect(page.getByText('重试标签49', { exact: true })).toBeVisible();
  expect(nextAttempts).toBe(2);
});

test('context directory exposes a retry when a resource refresh fails', async ({
  page
}) => {
  await mockApi(page);
  let releaseResourceEvent!: () => void;
  const resourceEvent = new Promise<void>((resolve) => {
    releaseResourceEvent = resolve;
  });
  let eventConnections = 0;
  await page.route('**/api/events', async (route) => {
    eventConnections += 1;
    if (eventConnections === 1) await resourceEvent;
    await route.fulfill({
      status: 200,
      headers: { 'Content-Type': 'text/event-stream' },
      body:
        eventConnections === 1
          ? 'event: app_event\ndata: {"resource":"work","resource_id":"work-1"}\n\n'
          : 'event: snapshot_refresh\ndata: {"latest_event_id":12}\n\n'
    });
  });

  let refreshed = false;
  let refreshAttempts = 0;
  await page.route('**/api/gallery/tags?*', async (route) => {
    if (refreshed) {
      refreshAttempts += 1;
      if (refreshAttempts === 1) {
        await route.fulfill({ status: 503 });
        return;
      }
    }
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        items: [
          {
            tag: {
              id: '0198f64c-42a2-7374-bace-000000001400',
              original: refreshed ? '刷新成功' : '刷新前',
              translation: null
            },
            work_count: 1,
            cover_url: refreshed
              ? '/context-thumbnail-after.svg'
              : '/context-thumbnail-before.svg',
            cover_width: 640,
            cover_height: 900,
            cover_age_rating: 'all_age'
          }
        ],
        total: 1,
        next_cursor: null
      })
    });
  });
  await page.route('**/context-thumbnail-*.svg', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'image/svg+xml',
      body: '<svg xmlns="http://www.w3.org/2000/svg" width="64" height="90"><rect width="64" height="90" fill="#0096fa"/></svg>'
    })
  );

  await page.goto('/gallery/tags');
  await expect(page.getByText('刷新前', { exact: true })).toBeVisible();
  await expect(page.locator('.context-card img')).toHaveAttribute(
    'src',
    '/context-thumbnail-before.svg'
  );
  refreshed = true;
  releaseResourceEvent();

  await expect(page.getByText('标签列表暂时无法读取')).toBeVisible();
  expect(refreshAttempts).toBe(1);
  await page.getByRole('button', { name: '重新加载' }).click();

  await expect(page.getByText('刷新成功', { exact: true })).toBeVisible();
  await expect(page.locator('.context-card img')).toHaveAttribute(
    'src',
    '/context-thumbnail-after.svg'
  );
  expect(refreshAttempts).toBe(2);
});

test('context directory lets a manual search replace a pending resource refresh', async ({
  page
}) => {
  await mockApi(page);
  await page.unroute('**/api/events');
  let publishWorkChange!: () => void;
  const workChange = new Promise<void>((resolve) => {
    publishWorkChange = resolve;
  });
  await page.route('**/api/events', async (route) => {
    await workChange;
    await route.fulfill({
      status: 200,
      headers: { 'Content-Type': 'text/event-stream' },
      body: 'event: app_event\ndata: {"resource":"work","resource_id":"work-1"}\n\n'
    });
  });
  let backgroundStarted!: () => void;
  const refreshStarted = new Promise<void>((resolve) => {
    backgroundStarted = resolve;
  });
  let releaseBackground!: () => void;
  const backgroundReleased = new Promise<void>((resolve) => {
    releaseBackground = resolve;
  });
  let unfilteredRequests = 0;
  await page.route('**/api/gallery/tags?*', async (route) => {
    const url = new URL(route.request().url());
    const manual = url.searchParams.get('q') === '手动搜索';
    if (!manual) {
      unfilteredRequests += 1;
      if (unfilteredRequests > 1) {
        backgroundStarted();
        await backgroundReleased;
      }
    }
    const name = manual
      ? '手动结果'
      : unfilteredRequests > 1
        ? '过期刷新结果'
        : '初始结果';
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        items: [
          {
            tag: {
              id: manual
                ? '0198f64c-42a2-7374-bace-000000001102'
                : '0198f64c-42a2-7374-bace-000000001101',
              original: name,
              translation: null
            },
            work_count: 1,
            cover_url: null,
            cover_width: null,
            cover_height: null
          }
        ],
        total: 1,
        next_cursor: null
      })
    });
  });

  await page.goto('/gallery/tags');
  await expect(page.getByText('初始结果', { exact: true })).toBeVisible();
  publishWorkChange();
  await refreshStarted;
  await page.getByPlaceholder('搜索标签').fill('手动搜索');
  await page.getByRole('button', { name: '搜索', exact: true }).click();
  await expect(page.getByText('手动结果', { exact: true })).toBeVisible();

  releaseBackground();
  await expect(page.getByText('手动结果', { exact: true })).toBeVisible();
  await expect(page.getByText('过期刷新结果', { exact: true })).toHaveCount(0);
});

test('context directory preserves its loaded depth during a resource refresh', async ({
  page
}) => {
  await page.setViewportSize({ width: 1000, height: 600 });
  await mockApi(page);
  let releaseResourceEvent!: () => void;
  const resourceEvent = new Promise<void>((resolve) => {
    releaseResourceEvent = resolve;
  });
  let eventConnections = 0;
  await page.route('**/api/events', async (route) => {
    eventConnections += 1;
    if (eventConnections === 1) await resourceEvent;
    await route.fulfill({
      status: 200,
      headers: { 'Content-Type': 'text/event-stream' },
      body:
        eventConnections === 1
          ? 'event: app_event\ndata: {"resource":"work","resource_id":"work-1"}\n\n'
          : 'event: snapshot_refresh\ndata: {"latest_event_id":12}\n\n'
    });
  });

  const tags = Array.from({ length: 160 }, (_, index) => ({
    tag: {
      id: `0198f64c-42a2-7374-bace-${(index + 1200)
        .toString(16)
        .padStart(12, '0')}`,
      original: `刷新标签${index + 1}`,
      translation: null
    },
    work_count: index + 1,
    cover_url: null,
    cover_width: null,
    cover_height: null
  }));
  tags[5]!.tag.original = '非常长的标签名称'.repeat(24);
  const addedTags = Array.from({ length: 8 }, (_, index) => ({
    tag: {
      id: `0198f64c-42a2-7374-bace-${(index + 0xffff)
        .toString(16)
        .padStart(12, '0')}`,
      original: `刷新后新增标签${index + 1}`,
      translation: null
    },
    work_count: 10_000 - index,
    cover_url: null,
    cover_width: null,
    cover_height: null
  }));
  let refreshed = false;
  let requests = 0;
  await page.route('**/api/gallery/tags?*', async (route) => {
    requests += 1;
    const current = refreshed ? [...addedTags, ...tags] : tags;
    const params = new URL(route.request().url()).searchParams;
    const cursor = Number(params.get('cursor') ?? 0);
    const limit = Number(params.get('limit') ?? 48);
    const items = current.slice(cursor, cursor + limit);
    const nextCursor = cursor + items.length;
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        items,
        total: current.length,
        next_cursor: nextCursor < current.length ? String(nextCursor) : null
      })
    });
  });

  await page.goto('/gallery/tags');
  const cards = page.locator('.context-card');
  await expect
    .poll(
      async () => {
        await page.evaluate(() =>
          window.scrollTo(0, document.body.scrollHeight)
        );
        return cards.count();
      },
      { timeout: 15_000 }
    )
    .toBe(tags.length);
  const anchor = cards.nth(100);
  await anchor.scrollIntoViewIfNeeded();
  const scrollBeforeRefresh = await page.evaluate(() => window.scrollY);
  const cardHeights = await cards.evaluateAll((elements) =>
    elements.map((element) => element.getBoundingClientRect().height)
  );
  expect(new Set(cardHeights).size).toBe(1);
  await page.evaluate(() => {
    const grid = document.querySelector('.context-grid');
    if (!grid) throw new Error('context grid is missing');
    const probe = {
      minimum: grid.querySelectorAll('.context-card').length
    };
    const observer = new MutationObserver(() => {
      probe.minimum = Math.min(
        probe.minimum,
        grid.querySelectorAll('.context-card').length
      );
    });
    observer.observe(grid, { childList: true });
    (
      window as typeof window & {
        contextRefreshProbe?: {
          probe: typeof probe;
          observer: MutationObserver;
        };
      }
    ).contextRefreshProbe = { probe, observer };
  });

  const requestsBeforeRefresh = requests;
  refreshed = true;
  releaseResourceEvent();
  await expect.poll(() => requests).toBeGreaterThan(requestsBeforeRefresh);
  await expect(
    page.getByText('刷新后新增标签1', { exact: true })
  ).toBeVisible();
  await expect(page.locator('.gallery-status')).toContainText('168个标签');
  await expect(cards.first()).toContainText('刷新后新增标签1');

  const result = await page.evaluate(() => {
    const state = (
      window as typeof window & {
        contextRefreshProbe?: {
          probe: { minimum: number };
          observer: MutationObserver;
        };
      }
    ).contextRefreshProbe;
    if (!state) throw new Error('context refresh probe is missing');
    state.observer.disconnect();
    return state.probe;
  });
  expect(result.minimum).toBe(tags.length);
  await expect
    .poll(async () =>
      Math.abs(
        (await page.evaluate(() => window.scrollY)) - scrollBeforeRefresh
      )
    )
    .toBeLessThan(2);
});

test('context directory reprojects a query selection when an in-flight refresh replaces visible cards', async ({
  page
}) => {
  await mockApi(page);
  await page.unroute('**/api/events');
  let publishWorkChange!: () => void;
  const workChange = new Promise<void>((resolve) => {
    publishWorkChange = resolve;
  });
  let eventConnections = 0;
  await page.route('**/api/events', async (route) => {
    eventConnections += 1;
    if (eventConnections === 1) await workChange;
    await route.fulfill({
      status: 200,
      headers: { 'Content-Type': 'text/event-stream' },
      body:
        eventConnections === 1
          ? 'event: app_event\ndata: {"resource":"work","resource_id":"work-1"}\n\n'
          : 'retry: 60000\n\n'
    });
  });

  const oldTagId = '0198f64c-42a2-7374-bace-000000001201';
  const newTagId = '0198f64c-42a2-7374-bace-000000001202';
  let refreshStarted!: () => void;
  const backgroundRefreshStarted = new Promise<void>((resolve) => {
    refreshStarted = resolve;
  });
  let releaseRefresh!: () => void;
  const backgroundRefreshReleased = new Promise<void>((resolve) => {
    releaseRefresh = resolve;
  });
  let directoryRequests = 0;
  await page.route('**/api/gallery/tags?*', async (route) => {
    directoryRequests += 1;
    if (directoryRequests > 1) {
      refreshStarted();
      await backgroundRefreshReleased;
    }
    const refreshed = directoryRequests > 1;
    await fulfillJson(route, 200, {
      items: [
        {
          tag: {
            id: refreshed ? newTagId : oldTagId,
            original: refreshed ? '刷新后新增标签' : '刷新后移除标签',
            translation: null
          },
          work_count: refreshed ? 7 : 5,
          cover_url: null,
          cover_width: null,
          cover_height: null
        }
      ],
      total: 4,
      next_cursor: null
    });
  });
  const projectionRequests: string[][] = [];
  await page.route('**/api/gallery/contexts/selection', async (route) => {
    const request = route.request().postDataJSON() as {
      visible_context_ids: string[];
    };
    projectionRequests.push([...request.visible_context_ids]);
    await fulfillJson(route, 200, {
      selected_context_count: 4,
      selected_work_count: 18,
      selected_visible_context_ids: request.visible_context_ids
    });
  });

  await page.goto('/gallery/tags');
  await expect(page.getByText('刷新后移除标签', { exact: true })).toBeVisible();
  publishWorkChange();
  await backgroundRefreshStarted;

  await page.getByRole('button', { name: '多选' }).click();
  await page.getByRole('button', { name: '全选', exact: true }).click();
  await expect(page.getByText('4个目录项已选择 · 18件作品')).toBeVisible();
  releaseRefresh();

  const refreshedCard = page
    .locator('.context-card')
    .filter({ hasText: '刷新后新增标签' });
  await expect(refreshedCard).toBeVisible();
  await expect(refreshedCard.getByRole('button')).toHaveAttribute(
    'aria-pressed',
    'true'
  );
  await expect(page.getByText('4个目录项已选择 · 18件作品')).toBeVisible();
  expect(projectionRequests).toEqual([[oldTagId], [newTagId]]);
});

test('context indexes show the shared unavailable thumbnail state', async ({
  page
}) => {
  await mockApi(page);
  const artist = {
    id: '0198f64c-42a2-7374-bace-000000001001',
    pixiv_artist_id: 2001,
    name: '无缩略图作者',
    account_name: null,
    work_count: 1,
    cover_url: null,
    cover_width: null,
    cover_height: null
  };
  const tag = {
    tag: {
      id: '0198f64c-42a2-7374-bace-000000001002',
      original: '损坏缩略图标签',
      translation: null
    },
    work_count: 1,
    cover_url: '/broken-context-thumbnail',
    cover_width: null,
    cover_height: null
  };
  const series = {
    id: '0198f64c-42a2-7374-bace-000000001003',
    pixiv_series_id: 5001,
    pixiv_artist_id: 2001,
    title: '无缩略图系列',
    work_count: 1,
    cover_url: null,
    cover_width: null,
    cover_height: null
  };
  for (const [endpoint, item] of [
    ['artists', artist],
    ['tags', tag],
    ['series', series]
  ] as const) {
    await page.route(`**/api/gallery/${endpoint}?*`, (route) =>
      route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({ items: [item], total: 1, next_cursor: null })
      })
    );
  }
  let brokenThumbnailRequests = 0;
  await page.route('**/broken-context-thumbnail*', (route) => {
    brokenThumbnailRequests += 1;
    return route.fulfill({
      status: 404,
      contentType: 'application/json',
      body: '{}'
    });
  });

  for (const [route, message] of [
    ['/gallery/artists', '缩略图不可用'],
    ['/gallery/tags', '缩略图未加载'],
    ['/gallery/series', '缩略图不可用']
  ] as const) {
    await page.goto(route);
    await expect(
      page.locator('.context-card').getByText(message)
    ).toBeVisible();
  }
  expect(brokenThumbnailRequests).toBe(2);
});

test('tag and series indexes distinguish empty data from empty search results', async ({
  page
}) => {
  await mockApi(page);
  const emptyPage = {
    items: [],
    total: 0,
    next_cursor: null
  };
  await page.route('**/api/gallery/tags?*', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify(emptyPage)
    })
  );
  await page.route('**/api/gallery/series?*', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify(emptyPage)
    })
  );

  await page.goto('/gallery/tags');
  await expect(page.getByText('还没有标签数据')).toBeVisible();
  await page.getByPlaceholder('搜索标签').fill('夜空');
  await page.getByRole('button', { name: '搜索', exact: true }).click();
  await expect(page.getByText('没有找到匹配的标签')).toBeVisible();

  await page.goto('/gallery/series');
  await expect(page.getByText('还没有系列数据')).toBeVisible();
  await page.getByPlaceholder('搜索系列').fill('画集');
  await page.getByRole('button', { name: '搜索', exact: true }).click();
  await expect(page.getByText('没有找到匹配的系列')).toBeVisible();
});

test('artist detail follows client-side route changes and ignores the older response', async ({
  page
}) => {
  await mockApi(page);
  const firstArtistId = '2001';
  const secondArtistId = '2002';
  let firstRequested = false;
  let releaseFirst: () => void;
  const firstReady = new Promise<void>((resolve) => {
    releaseFirst = resolve;
  });
  await page.route('**/api/gallery/artists/*', async (route) => {
    const id = new URL(route.request().url()).pathname.split('/').at(-1);
    if (id === firstArtistId) {
      firstRequested = true;
      await firstReady;
    }
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        id:
          id === firstArtistId
            ? '0198f64c-42a2-7374-bace-9f1c3b317fd1'
            : '0198f64c-42a2-7374-bace-9f1c3b317fd2',
        pixiv_artist_id: Number(id),
        name: id === firstArtistId ? '旧作者' : '当前作者',
        account_name: null,
        work_count: 0,
        cover_url: null,
        cover_width: null,
        cover_height: null
      })
    });
  });
  await page.route('**/api/gallery/search', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({ items: [], next_cursor: null })
    })
  );

  await page.goto(`/gallery/artists/${firstArtistId}`);
  await expect.poll(() => firstRequested).toBe(true);
  await page.evaluate((href) => {
    const link = document.createElement('a');
    link.href = href;
    document.body.append(link);
    link.click();
  }, `/gallery/artists/${secondArtistId}`);
  await expect(page.getByRole('heading', { name: '当前作者' })).toBeVisible();

  const olderResponse = page.waitForResponse((response) =>
    new URL(response.url()).pathname.endsWith(
      `/api/gallery/artists/${firstArtistId}`
    )
  );
  releaseFirst!();
  await olderResponse;
  await expect(page.getByRole('heading', { name: '当前作者' })).toBeVisible();
  await expect(page.getByRole('heading', { name: '旧作者' })).toHaveCount(0);
});

test('context details share Pixiv actions and author follow uses the verified state', async ({
  page
}) => {
  await mockApi(page);
  let followed = false;
  let followUpdate: unknown;
  await page.route('**/api/gallery/search', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({ items: [], next_cursor: null })
    })
  );
  await page.route('**/api/gallery/artists/*', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        id: ARTIST_ID,
        pixiv_artist_id: 2001,
        name: 'Sample Artist',
        account_name: null,
        work_count: 12,
        cover_url: null,
        cover_width: null,
        cover_height: null
      })
    })
  );
  await page.route('**/api/following/authors/2001/pixiv*', async (route) => {
    if (route.request().method() === 'PUT') {
      followUpdate = route.request().postDataJSON();
      followed = (followUpdate as { followed: boolean }).followed;
    }
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({ pixiv_artist_id: 2001, followed })
    });
  });

  await page.goto('/gallery/artists/2001');
  const authorActions = page.locator('.gallery-description-actions');
  await expect(authorActions.locator('button')).toHaveText('关注');
  await expect(authorActions.locator('a')).toHaveAttribute(
    'href',
    'https://www.pixiv.net/users/2001'
  );
  await expect(
    authorActions.locator('.artist-follow-action + a')
  ).toBeVisible();
  await authorActions.locator('button').click();
  await expect(authorActions.locator('button')).toHaveText('已关注');
  expect(followUpdate).toEqual({
    expected_account_id: PIXIV_ACCOUNT_ID,
    followed: true
  });

  await page.route('**/api/gallery/tags/*', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        tag: {
          id: '0198f64c-42a2-7374-bace-9f1c3b317fc1',
          original: '夜空',
          translation: null
        },
        work_count: 7,
        cover_url: null,
        cover_width: null,
        cover_height: null
      })
    })
  );
  await page.goto('/gallery/tags/%E5%A4%9C%E7%A9%BA');
  await expect(page.locator('.gallery-description-actions a')).toHaveAttribute(
    'href',
    'https://www.pixiv.net/tags/%E5%A4%9C%E7%A9%BA/artworks'
  );
  await expect(page.locator('.gallery-description-actions + p')).toContainText(
    '夜空'
  );

  await page.route('**/api/gallery/series/*', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        id: '0198f64c-42a2-7374-bace-9f1c3b317fc2',
        pixiv_series_id: 31001,
        pixiv_artist_id: 2001,
        title: '星空画集',
        work_count: 4,
        cover_url: null,
        cover_width: null,
        cover_height: null
      })
    })
  );
  await page.goto('/gallery/series/31001');
  await expect(page.locator('.gallery-description-actions a')).toHaveAttribute(
    'href',
    'https://www.pixiv.net/user/2001/series/31001'
  );
  await expect(page.locator('.gallery-description-actions + p')).toContainText(
    'Pixiv系列 31001'
  );
});

test('collected works without a cover are not labeled as metadata-only', async ({
  page
}) => {
  await mockApi(page);
  await page.route('**/api/gallery/search', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        items: [
          {
            ...galleryWork(),
            cover_available: false,
            cover_url: null,
            collection_state: 'collected',
            source_state: 'present'
          }
        ],
        next_cursor: null
      })
    })
  );

  await page.goto('/gallery');
  const card = page.locator('.gallery-card');
  await expect(page.getByText('封面不可用')).toBeVisible();
  await expect(card.getByText('仅有元数据')).toHaveCount(0);
  await expect(page.getByRole('button', { name: '采集原图' })).toHaveCount(0);
});

function galleryWork() {
  return {
    id: WORK_ID,
    pixiv_work_id: 1001,
    title: '夏夜の星',
    description: '夜空',
    artist_id: ARTIST_ID,
    pixiv_artist_id: 2001,
    artist_name: 'Sample Artist',
    series_id: null,
    series_title: null,
    work_kind: 'ugoira',
    age_rating: 'all_age',
    ai_generated: false,
    page_count: 1,
    collection_state: 'collected',
    source_state: 'present',
    bookmarked_by_current_account: false,
    bookmark_id: null,
    bookmark_count: 8200,
    view_count: 120000,
    like_count: 14000,
    comment_count: 72,
    pixiv_published_at: '2026-07-01T12:00:00Z',
    pixiv_updated_at: '2026-07-01T12:00:00Z',
    local_updated_at: '2026-07-30T12:00:00Z',
    cover_available: true,
    cover_url: '/api/derivatives/0198f64c-42a2-7374-bace-9f1c3b317fa3',
    cover_width: 640,
    cover_height: 900,
    media_kind: 'ugoira_zip',
    tags: [
      {
        id: '0198f64c-42a2-7374-bace-9f1c3b317fa4',
        original: '夜空',
        translation: null
      }
    ]
  };
}
