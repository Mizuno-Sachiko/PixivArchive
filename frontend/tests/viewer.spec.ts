import { expect, test } from '@playwright/test';
import { zipSync } from 'fflate';

import { fulfillJson, mockApi, mockPixivAccount } from './support';

const WORK_ID = '0198f64c-42a2-7374-bace-9f1c3b317fb1';
const MEDIA_ID = '0198f64c-42a2-7374-bace-9f1c3b317fb2';
const SECOND_MEDIA_ID = '0198f64c-42a2-7374-bace-9f1c3b317fb7';
const COVER_ID = '0198f64c-42a2-7374-bace-9f1c3b317fb8';
const FIRST_DERIVATIVE_ID = '0198f64c-42a2-7374-bace-9f1c3b317fba';
const SECOND_DERIVATIVE_ID = '0198f64c-42a2-7374-bace-9f1c3b317fbb';
const FIRST_DOMINANT_COLOR = '#2589d9';
const SECOND_DOMINANT_COLOR = '#42a86b';
const RESTORABLE_TRASH_CAPABILITIES = {
  can_restore: true,
  can_reschedule: true,
  blocked_reason: null
} as const;

test('work detail opens source media in the unified keyboard viewer', async ({
  page
}) => {
  await mockViewerApi(page, { kind: 'illustration' });

  await page.goto('/gallery/works/1002');
  await expect(page.getByRole('heading', { name: '蓝色花束' })).toBeVisible();
  await expect(page.locator('.pixiv-work-id')).toContainText('Pixiv ID');
  await expect(page.locator('.pixiv-work-id')).toContainText('1002');
  await expect(page.getByRole('button', { name: '隐藏' })).toHaveCount(0);
  await expect(page.getByText('查找相似图片')).toHaveCount(0);
  await expect(
    page.getByRole('button', { name: '收藏', exact: true })
  ).toHaveCount(0);
  const sourceLink = page
    .getByRole('heading', { name: '蓝色花束' })
    .locator('..')
    .getByRole('link', { name: '在Pixiv打开' });
  await expect(sourceLink).toHaveAttribute(
    'href',
    'https://www.pixiv.net/artworks/1002'
  );
  await expect(sourceLink).toHaveAttribute('target', '_blank');
  await expect(sourceLink).toHaveAttribute(
    'rel',
    'external noopener noreferrer'
  );
  await expect(sourceLink).toHaveText('');
  await expect(
    page.locator('.work-actions').getByRole('link', { name: '在Pixiv打开' })
  ).toHaveCount(0);
  await expect(sourceLink).toHaveAttribute('title', '在Pixiv打开');
  await expect(page.getByRole('link', { name: 'Sample Artist' })).toBeVisible();
  await expect(page.getByText('Pixiv UID：2002')).toHaveCount(0);
  await expect(page.getByRole('link', { name: '在Pixiv打开作者' })).toHaveCount(
    0
  );
  await expect(page.locator('.source-preview img')).toHaveAttribute(
    'src',
    `/api/media/${MEDIA_ID}/source`
  );
  const openViewer = page.getByRole('button', { name: '查看原图' });
  await openViewer.click();

  const viewer = page.getByRole('dialog', { name: '作品查看器' });
  await expect(viewer).toBeVisible();
  await expect(
    viewer.getByRole('img', { name: '蓝色花束 第1页' })
  ).toBeVisible();
  await expect(viewer.getByRole('button', { name: '适合视口' })).toHaveCount(0);
  const fitted = await viewer.locator('.viewer-stage').evaluate((stage) => {
    const image = stage.querySelector('img');
    if (!image) throw new Error('viewer image is missing');
    const stageBox = stage.getBoundingClientRect();
    const imageBox = image.getBoundingClientRect();
    return {
      stageWidth: stageBox.width,
      stageHeight: stageBox.height,
      imageWidth: imageBox.width,
      imageHeight: imageBox.height
    };
  });
  expect(fitted.imageWidth).toBeLessThanOrEqual(fitted.stageWidth);
  expect(fitted.imageHeight).toBeLessThanOrEqual(fitted.stageHeight);
  expect(
    Math.min(
      Math.abs(fitted.imageWidth - fitted.stageWidth),
      Math.abs(fitted.imageHeight - fitted.stageHeight)
    )
  ).toBeLessThan(1);
  const viewerBox = await viewer.boundingBox();
  if (!viewerBox) throw new Error('viewer is missing');
  await page.mouse.move(
    viewerBox.x + viewerBox.width / 2,
    viewerBox.y + viewerBox.height - 24
  );
  await viewer.getByRole('button', { name: '放大' }).click();
  await viewer.getByRole('button', { name: '放大' }).click();
  const stage = viewer.locator('.viewer-stage');
  await expect(stage).toHaveClass(/zoomed/);
  const stageBox = await stage.boundingBox();
  if (!stageBox) throw new Error('viewer stage is missing');
  await page.mouse.move(stageBox.x + stageBox.width / 2, stageBox.y + 80);
  await page.mouse.down();
  await page.mouse.move(
    stageBox.x + stageBox.width / 2,
    stageBox.y + stageBox.height - 80
  );
  await expect(stage).toHaveClass(/dragging/);
  await page.mouse.up();
  await expect(stage).not.toHaveClass(/dragging/);
  await expect
    .poll(() => dominantColor(viewer, '--viewer-dominant-color'))
    .toBe(FIRST_DOMINANT_COLOR);
  await expect(
    viewer.getByRole('link', { name: '在Pixiv打开' })
  ).toHaveAttribute('href', 'https://www.pixiv.net/artworks/1002');
  await viewer.getByRole('button', { name: '关闭查看器' }).focus();
  await page.keyboard.press('Shift+Tab');
  await expect(viewer.getByRole('link', { name: '在Pixiv打开' })).toBeFocused();
  await viewer.getByRole('button', { name: '下一页' }).click();
  await expect(
    viewer.getByRole('img', { name: '蓝色花束 第2页' })
  ).toBeVisible();
  await expect
    .poll(() => dominantColor(viewer, '--viewer-dominant-color'))
    .toBe(SECOND_DOMINANT_COLOR);
  await viewer.press('Escape');
  await expect(viewer).toBeHidden();
  await expect(openViewer).toBeFocused();
});

test('thumbnail masking leaves work sources and the unified viewer visible', async ({
  page
}) => {
  await mockViewerApi(page, {
    kind: 'illustration',
    ageRating: 'r18',
    maskNonAllAgeThumbnails: true
  });

  await page.goto('/gallery/works/1002');
  await expect(page.locator('.source-preview img')).toHaveAttribute(
    'src',
    `/api/media/${MEDIA_ID}/source`
  );
  await expect(
    page.locator('.page-strip [data-thumbnail-state="masked"]')
  ).toHaveCount(2);

  await page.getByRole('button', { name: '查看原图' }).click();
  const viewerImage = page
    .getByRole('dialog', { name: '作品查看器' })
    .getByRole('img', { name: '蓝色花束 第1页' });
  await expect(viewerImage).toBeVisible();
  await expect(viewerImage).toHaveAttribute(
    'src',
    `/api/media/${MEDIA_ID}/source`
  );
});

test('work page strip uses the shared unavailable state without a derivative', async ({
  page
}) => {
  await mockViewerApi(page, {
    kind: 'illustration',
    secondThumbnailAvailable: false
  });

  await page.goto('/gallery/works/1002');

  const secondPage = page.getByRole('button', { name: '查看第2页' });
  await expect(
    secondPage.locator('[data-thumbnail-state="unavailable"]')
  ).toBeVisible();
  await expect(secondPage.getByText('缩略图不可用')).toBeVisible();
});

test('viewer hides idle controls and wheel changes pages without revealing them', async ({
  page
}) => {
  await mockViewerApi(page, { kind: 'illustration' });
  await page.goto('/gallery/works/1002');
  await page.getByRole('button', { name: '查看原图' }).click();

  const viewer = page.getByRole('dialog', { name: '作品查看器' });
  await expect(viewer).toHaveClass(/controls-visible/);
  await expect(viewer).not.toHaveClass(/controls-visible/, { timeout: 3000 });

  const box = await viewer.boundingBox();
  if (!box) throw new Error('viewer is missing');
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.mouse.wheel(0, 180);
  await expect(
    viewer.getByRole('img', { name: '蓝色花束 第2页' })
  ).toBeVisible();
  await expect(viewer).not.toHaveClass(/controls-visible/);

  await page.mouse.move(box.x + box.width / 2, box.y + 24);
  await expect(viewer).toHaveClass(/controls-visible/);
});

test('viewer close icon is centered in its circular control', async ({
  page
}) => {
  await mockViewerApi(page, { kind: 'illustration' });
  await page.goto('/gallery/works/1002');
  await page.getByRole('button', { name: '查看原图' }).click();

  const close = page.getByRole('button', { name: '关闭查看器' });
  const icon = close.locator('svg');
  await expect(icon).toHaveCount(1);
  const centers = await close.evaluate((button) => {
    const svg = button.querySelector('svg');
    if (!svg) throw new Error('close icon is missing');
    const buttonBox = button.getBoundingClientRect();
    const iconBox = svg.getBoundingClientRect();
    return {
      buttonX: buttonBox.left + buttonBox.width / 2,
      buttonY: buttonBox.top + buttonBox.height / 2,
      iconX: iconBox.left + iconBox.width / 2,
      iconY: iconBox.top + iconBox.height / 2
    };
  });
  expect(Math.abs(centers.buttonX - centers.iconX)).toBeLessThanOrEqual(0.5);
  expect(Math.abs(centers.buttonY - centers.iconY)).toBeLessThanOrEqual(0.5);
});

test('viewer fullscreen control enters and exits the current viewer', async ({
  page
}) => {
  await mockViewerApi(page, { kind: 'illustration' });
  await page.goto('/gallery/works/1002');
  await page.getByRole('button', { name: '查看原图' }).click();

  const viewer = page.getByRole('dialog', { name: '作品查看器' });
  const fullscreenSurface = viewer.locator('.viewer-surface');
  const fullscreen = viewer.getByRole('button', { name: '进入全屏' });
  await expect(fullscreen).toHaveText('全屏');
  await fullscreen.click();
  await expect
    .poll(() =>
      fullscreenSurface.evaluate(
        (element) => document.fullscreenElement === element
      )
    )
    .toBe(true);
  await expect(viewer.getByRole('button', { name: '退出全屏' })).toHaveText(
    '退出全屏'
  );

  await viewer.getByRole('button', { name: '退出全屏' }).click();
  await expect
    .poll(() => page.evaluate(() => document.fullscreenElement === null))
    .toBe(true);
  await expect(viewer.getByRole('button', { name: '进入全屏' })).toHaveText(
    '全屏'
  );
});

test('viewer keeps a usable fullscreen control when permission is denied', async ({
  page
}) => {
  await mockViewerApi(page, { kind: 'illustration' });
  await page.goto('/gallery/works/1002');
  await page.getByRole('button', { name: '查看原图' }).click();

  const pageErrors: Error[] = [];
  page.on('pageerror', (error) => pageErrors.push(error));
  await page.evaluate(() => {
    Object.defineProperty(Element.prototype, 'requestFullscreen', {
      configurable: true,
      value: () => Promise.reject(new DOMException('denied', 'NotAllowedError'))
    });
  });

  const viewer = page.getByRole('dialog', { name: '作品查看器' });
  const fullscreen = viewer.getByRole('button', { name: '进入全屏' });
  await fullscreen.click();
  await expect(fullscreen).toBeEnabled();
  await expect(fullscreen).toHaveText('全屏');
  await expect
    .poll(() => page.evaluate(() => document.fullscreenElement === null))
    .toBe(true);
  expect(pageErrors).toEqual([]);
});

test('work detail keeps the viewer open while fresh data is loading', async ({
  page
}) => {
  await mockViewerApi(page, { kind: 'illustration' });
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
      body: `retry: 60000\nevent: app_event\ndata: {"resource":"work","resource_id":"${WORK_ID}"}\n\n`
    });
  });

  let detailRequests = 0;
  let announceRefresh!: () => void;
  const refreshStarted = new Promise<void>((resolve) => {
    announceRefresh = resolve;
  });
  let releaseRefresh!: () => void;
  const refreshGate = new Promise<void>((resolve) => {
    releaseRefresh = resolve;
  });
  await page.route(`**/api/works/${WORK_ID}`, async (route) => {
    detailRequests += 1;
    if (detailRequests > 1) {
      announceRefresh();
      await refreshGate;
    }
    await route.fallback();
  });

  await page.goto('/gallery/works/1002');
  await page.getByRole('button', { name: '查看原图' }).click();
  const viewer = page.getByRole('dialog', { name: '作品查看器' });
  await expect(viewer).toBeVisible();

  publishWorkChange();
  await refreshStarted;
  await expect(viewer).toBeVisible();

  releaseRefresh();
  await expect(viewer).toBeVisible();
});

test('trashed work detail keeps local media and exposes trash actions', async ({
  page
}) => {
  await mockViewerApi(page, {
    kind: 'illustration',
    bookmarkWriteback: true,
    collectionState: 'trash'
  });
  const commands: string[] = [];
  await page.route(`**/api/trash/${WORK_ID}/purge`, async (route) => {
    commands.push('purge');
    await fulfillJson(route, 202, {
      job_id: '0198f64c-42a2-7374-bace-9f1c3b317fbc'
    });
  });

  await page.goto('/gallery/works/1002');

  await expect(page.getByRole('heading', { name: '蓝色花束' })).toBeVisible();
  await expect(page.locator('.source-preview img')).toHaveAttribute(
    'src',
    `/api/media/${MEDIA_ID}/source`
  );
  await expect(
    page.getByRole('button', { name: '收藏', exact: true })
  ).toHaveCount(0);
  await expect(page.getByRole('button', { name: '移入回收站' })).toHaveCount(0);
  await expect(page.getByRole('button', { name: '移出回收站' })).toBeVisible();

  await page.getByRole('button', { name: '立即清理' }).click();
  const confirmation = page.getByRole('dialog', { name: '立即清理作品' });
  await expect(confirmation).toContainText('蓝色花束');
  await confirmation.getByRole('button', { name: '立即清理' }).click();
  await expect(page.getByText('作品已加入后台清理队列')).toBeVisible();
  expect(commands).toEqual(['purge']);
});

test('restoring a trashed work returns to its trash snapshot without reloading detail', async ({
  page
}) => {
  await mockViewerApi(page, {
    kind: 'illustration',
    collectionState: 'trash'
  });
  let restoreRequests = 0;
  let detailRequests = 0;
  let restored = false;
  page.on('request', (request) => {
    if (new URL(request.url()).pathname === `/api/works/${WORK_ID}`) {
      detailRequests += 1;
    }
  });
  await page.route(`**/api/trash/${WORK_ID}/restore`, async (route) => {
    restoreRequests += 1;
    restored = true;
    await route.fulfill({ status: 204 });
  });
  await page.route('**/api/trash?*', (route) =>
    fulfillJson(route, 200, {
      items: restored
        ? []
        : [
            {
              work_id: WORK_ID,
              pixiv_work_id: 1002,
              title: '蓝色花束',
              artist_name: 'Sample Artist',
              page_count: 2,
              previous_collection_state: 'collected',
              trashed_at: '2026-08-12T10:00:00Z',
              scheduled_purge_at: '2026-09-11T10:00:00Z',
              purge_state: 'pending',
              purge_attempts: 0,
              failure_message: null,
              capabilities: RESTORABLE_TRASH_CAPABILITIES,
              estimated_release_bytes: 1024
            }
          ],
      next_cursor: null,
      summary: {
        total_count: restored ? 0 : 1,
        logical_bytes: restored ? 0 : 1024,
        estimated_reclaimable_bytes: restored ? 0 : 1024
      },
      all_summary: {
        total_count: restored ? 0 : 1,
        logical_bytes: restored ? 0 : 1024,
        estimated_reclaimable_bytes: restored ? 0 : 1024
      }
    })
  );

  await page.goto('/system/trash');
  await page.getByRole('link', { name: '查看蓝色花束详情' }).click();
  await expect(page).toHaveURL('/gallery/works/1002');
  await expect(page.getByRole('heading', { name: '蓝色花束' })).toBeVisible();
  const detailRequestsBeforeRestore = detailRequests;
  await page.getByRole('button', { name: '移出回收站' }).click();

  await expect(page).toHaveURL('/system/trash');
  await expect(page.getByRole('heading', { name: '回收站' })).toBeVisible();
  await expect(page.getByText('蓝色花束', { exact: true })).toHaveCount(0);
  expect(restoreRequests).toBe(1);
  expect(detailRequests).toBe(detailRequestsBeforeRestore);
});

test('work detail return source belongs to the current navigation and expires after reload', async ({
  page
}) => {
  await mockViewerApi(page, {
    kind: 'illustration',
    bookmarkWriteback: true
  });
  await page.route('**/api/gallery/search', (route) =>
    fulfillJson(route, 200, {
      items: [workSummary('illustration')],
      next_cursor: null
    })
  );
  await page.route('**/api/gallery/count', (route) =>
    fulfillJson(route, 200, { count: 1 })
  );

  await page.goto('/gallery/favorites');
  await page.getByRole('link', { name: '打开蓝色花束' }).click();
  const returnLink = page
    .getByRole('navigation', { name: '二级导航' })
    .getByRole('link', { name: '返回' });
  await expect(returnLink).toHaveAttribute('href', '/gallery/favorites');

  await returnLink.click();
  await expect(page).toHaveURL('/gallery/favorites');

  await page.goBack();
  await expect(page).toHaveURL('/gallery/works/1002');
  await expect(
    page
      .getByRole('navigation', { name: '二级导航' })
      .getByRole('link', { name: '返回' })
  ).toHaveAttribute('href', '/gallery');
  await page.goBack();
  await expect(page).toHaveURL('/gallery/favorites');

  await page.evaluate(() => {
    const link = document.createElement('a');
    link.href = '/gallery/works/1002';
    link.textContent = '无来源详情入口';
    document.body.append(link);
    link.click();
  });
  await expect(page).toHaveURL('/gallery/works/1002');
  await expect(
    page
      .getByRole('navigation', { name: '二级导航' })
      .getByRole('link', { name: '返回' })
  ).toHaveAttribute('href', '/gallery');

  await page.reload();
  await expect(
    page
      .getByRole('navigation', { name: '二级导航' })
      .getByRole('link', { name: '返回' })
  ).toHaveAttribute('href', '/gallery');
});

test('moving a work to trash returns to the gallery snapshot', async ({
  page
}) => {
  await mockViewerApi(page, { kind: 'illustration' });
  await page.route('**/api/gallery/search', async (route) => {
    const search = route.request().postDataJSON() as {
      groups?: Array<{ filters?: Array<{ type?: string }> }>;
    };
    const resolvingWork = search.groups?.some((group) =>
      group.filters?.some((filter) => filter.type === 'pixiv_work_id')
    );
    await fulfillJson(route, 200, {
      items: [workSummary('illustration')],
      next_cursor: null,
      ...(resolvingWork ? {} : { total_count: 1 })
    });
  });
  await page.route('**/api/gallery/count', (route) =>
    fulfillJson(route, 200, { count: 1 })
  );
  await page.route('**/api/works/*/trash', (route) =>
    fulfillJson(route, 200, {
      work_id: WORK_ID,
      previous_collection_state: 'collected',
      trashed_at: '2026-08-12T10:00:00Z',
      scheduled_purge_at: '2026-09-11T10:00:00Z',
      purge_state: 'pending',
      purge_attempts: 0,
      failure_message: null
    })
  );

  await page.goto('/gallery');
  await page.getByRole('link', { name: '打开蓝色花束' }).click();
  await expect(page).toHaveURL('/gallery/works/1002');
  await page.getByRole('button', { name: '移入回收站' }).click();

  await expect(page).toHaveURL('/gallery');
  await expect(page.getByRole('heading', { name: '图库' })).toBeVisible();
});

test('missing work uses a stable shared alert with a gallery return action', async ({
  page
}) => {
  await mockViewerApi(page, { kind: 'illustration' });
  await page.unroute('**/api/events');
  let resolutionRequests = 0;
  let reportRefreshStarted!: () => void;
  const refreshStarted = new Promise<void>((resolve) => {
    reportRefreshStarted = resolve;
  });
  let releaseRefresh!: () => void;
  const refreshReleased = new Promise<void>((resolve) => {
    releaseRefresh = resolve;
  });
  await page.route('**/api/works/by-pixiv-id/999999999', async (route) => {
    resolutionRequests += 1;
    if (resolutionRequests === 1) {
      await fulfillJson(route, 404, { error: 'not_found' });
      return;
    }
    reportRefreshStarted();
    await refreshReleased;
    await fulfillJson(route, 503, { error: 'unavailable' });
  });
  let publishWorkChange!: () => void;
  const workChange = new Promise<void>((resolve) => {
    publishWorkChange = resolve;
  });
  await page.route('**/api/events', async (route) => {
    await workChange;
    await route.fulfill({
      status: 200,
      headers: { 'Content-Type': 'text/event-stream' },
      body: `retry: 60000\nevent: app_event\ndata: {"resource":"work","resource_id":"${WORK_ID}"}\n\n`
    });
  });

  await page.goto('/gallery/works/999999999');
  const alert = page.locator('.alert-banner');
  await expect(alert).toContainText('没有找到这个Pixiv作品');
  await expect(alert.getByRole('link', { name: '返回图库' })).toHaveAttribute(
    'href',
    '/gallery'
  );
  const alertAlignment = await alert.evaluate((element) => {
    const icon = element.querySelector('.icon');
    const title = element.querySelector('strong');
    const message = element.querySelector('p');
    if (!icon || !title || !message) {
      throw new Error('alert banner content is incomplete');
    }
    const iconBox = icon.getBoundingClientRect();
    const titleBox = title.getBoundingClientRect();
    const messageBox = message.getBoundingClientRect();
    return {
      icon: iconBox.top + iconBox.height / 2,
      copy: (titleBox.top + messageBox.bottom) / 2
    };
  });
  expect(
    Math.abs(alertAlignment.icon - alertAlignment.copy)
  ).toBeLessThanOrEqual(1);

  publishWorkChange();
  await refreshStarted;
  try {
    await expect(alert).toContainText('没有找到这个Pixiv作品');
    await expect(alert).toBeVisible();
  } finally {
    releaseRefresh();
  }
  await expect.poll(() => resolutionRequests).toBeGreaterThan(1);
  await expect(alert).toContainText('没有找到这个Pixiv作品');
  await expect(page.getByText('作品详情暂时无法读取')).toHaveCount(0);
  await expect(
    page.getByRole('button', { name: '重新读取作品详情' })
  ).toHaveCount(0);
  await alert.getByRole('link', { name: '返回图库' }).click();
  await expect(page).toHaveURL('/gallery');
  await expect(page.getByRole('heading', { name: '图库' })).toBeVisible();
});

test('work detail retries initial and cached refresh failures', async ({
  page
}) => {
  await mockViewerApi(page, { kind: 'illustration' });
  let detailUnavailable = true;
  await page.route(`**/api/works/${WORK_ID}`, async (route) => {
    if (detailUnavailable) {
      await fulfillJson(route, 503, { error: 'unavailable' });
      return;
    }
    await route.fallback();
  });

  await page.goto('/gallery/works/1002');
  await expect(page.getByText('作品详情暂时无法读取')).toBeVisible();

  detailUnavailable = false;
  await page.getByRole('button', { name: '重新读取作品详情' }).click();
  await expect(page.getByRole('heading', { name: '蓝色花束' })).toBeVisible();
  await expect(
    page.getByRole('button', { name: '重新读取作品详情' })
  ).toHaveCount(0);

  detailUnavailable = true;
  await page.evaluate(() =>
    document.dispatchEvent(new Event('visibilitychange'))
  );
  await expect(
    page.getByText('作品详情更新失败，当前仍显示上次读取的数据')
  ).toBeVisible();
  await expect(page.getByRole('heading', { name: '蓝色花束' })).toBeVisible();

  detailUnavailable = false;
  await page.getByRole('button', { name: '重新读取作品详情' }).click();
  await expect(
    page.getByText('作品详情更新失败，当前仍显示上次读取的数据')
  ).toHaveCount(0);
  await expect(
    page.getByRole('button', { name: '重新读取作品详情' })
  ).toHaveCount(0);
});

test('work detail clears account-scoped data when the Pixiv account changes', async ({
  page
}) => {
  await mockViewerApi(page, {
    kind: 'illustration',
    bookmarkWriteback: true
  });
  let switched = false;
  let accountLoads = 0;
  const accountA = mockPixivAccount({
    account_id: '0198f64c-42a2-7374-bace-9f1c3b317fb6',
    display_name: 'Account A',
    bookmark_writeback_enabled: true
  });
  const accountB = mockPixivAccount({
    account_id: '0198f64c-42a2-7374-bace-9f1c3b317fb9',
    display_name: 'Account B',
    bookmark_writeback_enabled: true
  });

  await page.route('**/api/pixiv/account', async (route) => {
    accountLoads += 1;
    await fulfillJson(route, 200, switched ? accountB : accountA);
  });
  await page.route(`**/api/works/${WORK_ID}`, async (route) => {
    if (switched) {
      await fulfillJson(route, 503, { error: 'unavailable' });
      return;
    }
    await route.fallback();
  });

  await page.goto('/gallery/works/1002');
  await expect(page.getByRole('heading', { name: '蓝色花束' })).toBeVisible();

  switched = true;
  await page.evaluate(() =>
    document.dispatchEvent(new Event('visibilitychange'))
  );
  await expect.poll(() => accountLoads).toBeGreaterThan(1);
  await expect(page.getByRole('heading', { name: '蓝色花束' })).toHaveCount(0);
  await expect(page.getByText('作品详情暂时无法读取')).toBeVisible();
  await expect(
    page.getByText('作品详情更新失败，当前仍显示上次读取的数据')
  ).toHaveCount(0);
});

test('work detail hides the account bookmark action when the same account becomes unavailable', async ({
  page
}) => {
  await mockViewerApi(page, {
    kind: 'illustration',
    bookmarkWriteback: true
  });
  let unavailable = false;
  let accountLoads = 0;
  let detailLoads = 0;
  const accountId = '0198f64c-42a2-7374-bace-9f1c3b317fb6';

  await page.route('**/api/pixiv/account', async (route) => {
    accountLoads += 1;
    await fulfillJson(
      route,
      200,
      mockPixivAccount({
        account_id: accountId,
        display_name: 'Account A',
        state: unavailable ? 'credential_invalid' : 'normal',
        bookmark_writeback_enabled: true,
        revision: unavailable ? 2 : 1
      })
    );
  });
  await page.route(`**/api/works/${WORK_ID}`, async (route) => {
    detailLoads += 1;
    await route.fallback();
  });

  await page.goto('/gallery/works/1002');
  await expect(
    page.getByRole('button', { name: '收藏', exact: true })
  ).toBeVisible();
  await expect(page.getByText('3200', { exact: true })).toBeVisible();
  const initialDetailLoads = detailLoads;

  unavailable = true;
  await page.evaluate(() =>
    document.dispatchEvent(new Event('visibilitychange'))
  );
  await expect.poll(() => accountLoads).toBeGreaterThan(1);
  await expect.poll(() => detailLoads).toBeGreaterThan(initialDetailLoads);
  await expect(
    page.getByRole('button', { name: '收藏', exact: true })
  ).toHaveCount(0);
  await expect(page.getByText('3200', { exact: true })).toBeVisible();
});

test('page thumbnails and wheel change the detail page without opening the viewer', async ({
  page
}) => {
  await mockViewerApi(page, { kind: 'illustration' });

  await page.goto('/gallery/works/1002');
  const preview = page.locator('.source-preview');
  const image = preview.getByRole('img');
  await expect(image).toHaveAttribute('src', `/api/media/${MEDIA_ID}/source`);
  await expect
    .poll(() => dominantColor(preview, '--source-dominant-color'))
    .toBe(FIRST_DOMINANT_COLOR);

  await page.getByRole('button', { name: '查看第2页' }).click();
  await expect(image).toHaveAttribute(
    'src',
    `/api/media/${SECOND_MEDIA_ID}/source`
  );
  await expect
    .poll(() => dominantColor(preview, '--source-dominant-color'))
    .toBe(SECOND_DOMINANT_COLOR);
  await expect(page.getByRole('dialog', { name: '作品查看器' })).toHaveCount(0);

  await page.getByRole('button', { name: '查看第1页' }).click();
  const scrollBefore = await page.evaluate(() => window.scrollY);
  await preview.hover();
  await page.mouse.wheel(0, 180);
  await expect(image).toHaveAttribute(
    'src',
    `/api/media/${SECOND_MEDIA_ID}/source`
  );
  expect(await page.evaluate(() => window.scrollY)).toBe(scrollBefore);

  await page.getByRole('button', { name: '查看原图' }).click();
  await expect(
    page
      .getByRole('dialog', { name: '作品查看器' })
      .getByRole('img', { name: '蓝色花束 第2页' })
  ).toBeVisible();
});

test('page strip follows the active thumbnail only when it reaches an edge', async ({
  page
}) => {
  await mockViewerApi(page, { kind: 'illustration' });
  await page.goto('/gallery/works/1002');

  const strip = page.locator('.page-strip');
  const preview = page.locator('.source-preview');
  const secondPage = page.getByRole('button', { name: '查看第2页' });
  const documentScroll = await page.evaluate(() => window.scrollY);
  await strip.evaluate((element) => {
    const target = element as HTMLElement;
    target.style.width = '150px';
    target.style.paddingRight = '100px';
    target.scrollLeft = 20;
  });
  await preview.hover();
  await page.mouse.wheel(0, 180);
  await expect(secondPage).toHaveAttribute('aria-pressed', 'true');
  await expect
    .poll(() =>
      strip.evaluate((element) => (element as HTMLElement).scrollLeft)
    )
    .toBe(20);

  await page.mouse.wheel(0, -180);
  await expect
    .poll(() =>
      strip.evaluate((element) => (element as HTMLElement).scrollLeft)
    )
    .toBe(0);
  await strip.evaluate((element) => {
    const target = element as HTMLElement;
    target.style.width = '80px';
    target.style.paddingRight = '0';
  });
  const rightOverflow = await strip.evaluate((element) => {
    const stripBounds = element.getBoundingClientRect();
    const second = element.querySelector<HTMLElement>('[data-page-index="1"]');
    return second!.getBoundingClientRect().right - stripBounds.right;
  });

  await page.mouse.wheel(0, 180);
  await expect(secondPage).toHaveAttribute('aria-pressed', 'true');
  await expect
    .poll(() =>
      strip.evaluate((element) => (element as HTMLElement).scrollLeft)
    )
    .toBeGreaterThan(0);
  const afterRight = await strip.evaluate((element) => {
    const stripBounds = element.getBoundingClientRect();
    const active = element.querySelector<HTMLElement>('[aria-pressed="true"]');
    const activeBounds = active!.getBoundingClientRect();
    return {
      scrollLeft: (element as HTMLElement).scrollLeft,
      stripLeft: stripBounds.left,
      stripRight: stripBounds.right,
      activeLeft: activeBounds.left,
      activeRight: activeBounds.right
    };
  });
  expect(Math.abs(afterRight.scrollLeft - rightOverflow)).toBeLessThanOrEqual(
    1
  );
  expect(afterRight.activeLeft).toBeGreaterThanOrEqual(
    afterRight.stripLeft - 1
  );
  expect(afterRight.activeRight).toBeLessThanOrEqual(afterRight.stripRight + 1);

  await page.mouse.wheel(0, -180);
  await expect
    .poll(() =>
      strip.evaluate((element) => (element as HTMLElement).scrollLeft)
    )
    .toBe(0);
  const afterLeft = await strip.evaluate((element) => {
    const stripBounds = element.getBoundingClientRect();
    const active = element.querySelector<HTMLElement>('[aria-pressed="true"]');
    const activeBounds = active!.getBoundingClientRect();
    return {
      stripLeft: stripBounds.left,
      stripRight: stripBounds.right,
      activeLeft: activeBounds.left,
      activeRight: activeBounds.right
    };
  });
  expect(afterLeft.activeLeft).toBeGreaterThanOrEqual(afterLeft.stripLeft - 1);
  expect(afterLeft.activeRight).toBeLessThanOrEqual(afterLeft.stripRight + 1);
  expect(await page.evaluate(() => window.scrollY)).toBe(documentScroll);
});

test('work descriptions keep readable safe Pixiv content', async ({ page }) => {
  await mockViewerApi(page, {
    kind: 'illustration',
    description:
      '<p style="font-size:36px;color:red">第一段<br>第二行</p><p><a href="https://example.com/path" style="color:red">安全链接</a><img src="https://example.com/tracker.png"><script>危险脚本</script></p>'
  });

  await page.goto('/gallery/works/1002');
  const description = page.locator('.description');
  await expect(description).toContainText('第一段');
  await expect(description).toContainText('第二行');
  await expect(description).not.toContainText('危险脚本');
  await expect(description.locator('img, script')).toHaveCount(0);
  const link = description.getByRole('link', { name: '安全链接' });
  await expect(link).toHaveAttribute('href', 'https://example.com/path');
  await expect(link).toHaveAttribute('rel', 'noopener noreferrer');
  await expect(link).not.toHaveAttribute('style');
});

test('work detail formats timestamps in the browser timezone', async ({
  browser
}) => {
  const context = await browser.newContext({ timezoneId: 'America/New_York' });
  const page = await context.newPage();
  try {
    await mockViewerApi(page, {
      kind: 'illustration',
      revisionPageCount: 42,
      revisionSourceName: '关注动态',
      revisionSourceAccountId: 810004
    });
    await page.goto('/gallery/works/1002');

    const publishedTime = page.locator('.published-time time');
    await expect(publishedTime).toContainText('2026年7月1日 08:00:00');
    await expect(publishedTime).toHaveAttribute('title', /2026/);
    const revisionTime = page.locator('.revision-list time');
    await expect(revisionTime).toContainText('2026年7月30日 08:00:00');
    await expect(revisionTime).toHaveAttribute('title', /2026/);
    const source = page.locator('.revision-source');
    const meta = page.locator('.revision-meta');
    await expect(meta).toHaveText('42页 · illustration');
    await expect(source).toContainText('来自：关注动态 · 账户810004');
    const [revisionTimeBox, metaBox, sourceBox] = await Promise.all([
      page.locator('.revision-time').boundingBox(),
      meta.boundingBox(),
      source.boundingBox()
    ]);
    expect(revisionTimeBox).not.toBeNull();
    expect(metaBox).not.toBeNull();
    expect(sourceBox).not.toBeNull();
    expect(
      Math.abs(
        revisionTimeBox!.x +
          revisionTimeBox!.width -
          (sourceBox!.x + sourceBox!.width)
      )
    ).toBeLessThanOrEqual(1);
    expect(Math.abs(metaBox!.y - sourceBox!.y)).toBeLessThanOrEqual(1);
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= window.innerWidth
      )
    ).toBe(true);
  } finally {
    await context.close();
  }
});

test('long revision sources wrap without horizontal overflow', async ({
  page
}) => {
  await mockViewerApi(page, { kind: 'illustration' });
  await page.goto('/gallery/works/1002');

  const details = page.locator('.revision-details');
  await details.evaluate((element) => {
    (element as HTMLElement).style.width = '240px';
  });
  const [metaBox, sourceBox] = await Promise.all([
    details.locator('.revision-meta').boundingBox(),
    details.locator('.revision-source').boundingBox()
  ]);
  expect(metaBox).not.toBeNull();
  expect(sourceBox).not.toBeNull();
  expect(sourceBox!.y).toBeGreaterThan(metaBox!.y);
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= window.innerWidth
    )
  ).toBe(true);
});

test('Ugoira uses the same viewer shell with autoplay and one play control', async ({
  page
}) => {
  await mockViewerApi(page, { kind: 'ugoira' });

  await page.goto('/gallery/works/1002');
  await expect(page.getByText('动图', { exact: true }).first()).toBeVisible();
  await expect(
    page.locator('.source-preview').getByLabel('动图画面')
  ).toBeVisible();
  await page.getByRole('button', { name: '查看原图' }).click();

  const viewer = page.getByRole('dialog', { name: '作品查看器' });
  const viewerBox = await viewer.boundingBox();
  if (!viewerBox) throw new Error('viewer is missing');
  await page.mouse.move(
    viewerBox.x + viewerBox.width / 2,
    viewerBox.y + viewerBox.height - 24
  );
  await expect(viewer.getByRole('button', { name: '暂停动图' })).toBeVisible();
  await viewer.getByRole('button', { name: '暂停动图' }).click();
  await expect(viewer.getByRole('button', { name: '播放动图' })).toBeVisible();
});

test('work detail shows the Pixiv bookmark action when writeback is enabled', async ({
  page
}) => {
  await mockViewerApi(page, {
    kind: 'illustration',
    bookmarkWriteback: true
  });

  await page.goto('/gallery/works/1002');
  await expect(
    page.getByRole('button', { name: '收藏', exact: true })
  ).toBeVisible();
});

test('work detail never widens the document viewport', async ({ page }) => {
  await page.setViewportSize({ width: 1100, height: 720 });
  await mockViewerApi(page, {
    kind: 'illustration',
    description: `长链接 https://example.com/${'very-long-segment'.repeat(30)}`
  });

  await page.goto('/gallery/works/1002');
  await expect(page.locator('.work-detail')).toBeVisible();
  const overflow = await page.evaluate(
    () =>
      document.documentElement.scrollWidth -
      document.documentElement.clientWidth
  );
  expect(overflow).toBeLessThanOrEqual(0);
});

interface ViewerMockOptions {
  kind: 'illustration' | 'ugoira';
  bookmarkWriteback?: boolean;
  description?: string;
  ageRating?: 'all_age' | 'r18' | 'r18g' | 'unknown';
  maskNonAllAgeThumbnails?: boolean;
  secondThumbnailAvailable?: boolean;
  collectionState?: 'collected' | 'metadata_only' | 'trash';
  revisionPageCount?: number;
  revisionSourceName?: string;
  revisionSourceAccountId?: number;
}

async function mockViewerApi(
  page: import('@playwright/test').Page,
  {
    kind,
    bookmarkWriteback = false,
    description = '作品描述',
    ageRating = 'all_age',
    maskNonAllAgeThumbnails = false,
    secondThumbnailAvailable = true,
    collectionState = 'collected',
    revisionPageCount = 1,
    revisionSourceName = '首页精选与长期收藏同步来源名称很长',
    revisionSourceAccountId = 2002
  }: ViewerMockOptions
): Promise<{
  setCollectionState: (
    value: NonNullable<ViewerMockOptions['collectionState']>
  ) => void;
}> {
  let currentCollectionState = collectionState;
  await mockApi(page);
  await page.route('**/api/works/by-pixiv-id/1002', (route) =>
    fulfillJson(route, 200, { work_id: WORK_ID })
  );
  await page.route('**/api/gallery/search', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        items: [
          workSummary(kind, description, ageRating, currentCollectionState)
        ],
        next_cursor: null
      })
    })
  );
  await page.route(`**/api/works/${WORK_ID}`, (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        work: workSummary(kind, description, ageRating, currentCollectionState),
        pages: [
          {
            id: '0198f64c-42a2-7374-bace-9f1c3b317fb3',
            page_index: 0,
            source_state: 'present',
            width: 800,
            height: 1200,
            current_media: {
              id: MEDIA_ID,
              revision_number: 1,
              media_kind: kind === 'ugoira' ? 'ugoira_zip' : 'source_image',
              format: kind === 'ugoira' ? 'zip' : 'png',
              byte_size: 1024,
              sha256: '00'.repeat(32),
              source_url: `/api/media/${MEDIA_ID}/source`,
              derivatives: [
                {
                  id: FIRST_DERIVATIVE_ID,
                  kind: 'waterfall',
                  format: 'webp',
                  width: 800,
                  height: 1200,
                  byte_size: 512,
                  dominant_color: FIRST_DOMINANT_COLOR,
                  url: `/api/derivatives/${FIRST_DERIVATIVE_ID}`
                }
              ]
            }
          },
          ...(kind === 'illustration'
            ? [
                {
                  id: '0198f64c-42a2-7374-bace-9f1c3b317fb9',
                  page_index: 1,
                  source_state: 'present',
                  width: 1200,
                  height: 800,
                  current_media: {
                    id: SECOND_MEDIA_ID,
                    revision_number: 1,
                    media_kind: 'source_image',
                    format: 'png',
                    byte_size: 2048,
                    sha256: '11'.repeat(32),
                    source_url: `/api/media/${SECOND_MEDIA_ID}/source`,
                    derivatives: secondThumbnailAvailable
                      ? [
                          {
                            id: SECOND_DERIVATIVE_ID,
                            kind: 'waterfall',
                            format: 'webp',
                            width: 1200,
                            height: 800,
                            byte_size: 768,
                            dominant_color: SECOND_DOMINANT_COLOR,
                            url: `/api/derivatives/${SECOND_DERIVATIVE_ID}`
                          }
                        ]
                      : []
                  }
                }
              ]
            : [])
        ],
        ugoira:
          kind === 'ugoira'
            ? {
                frame_mime_type: 'image/png',
                frames: [
                  { file: '000000.png', delay_ms: 100 },
                  { file: '000001.png', delay_ms: 120 }
                ]
              }
            : null,
        trash_capabilities:
          currentCollectionState === 'trash'
            ? RESTORABLE_TRASH_CAPABILITIES
            : null
      })
    })
  );
  await page.route(`**/api/works/${WORK_ID}/revisions`, (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify([
        {
          id: '0198f64c-42a2-7374-bace-9f1c3b317fb4',
          title: '蓝色花束',
          description: null,
          work_kind: kind,
          page_count: revisionPageCount,
          captured_at: '2026-07-30T12:00:00Z',
          sources: [
            {
              subscription_name: revisionSourceName,
              pixiv_user_id: revisionSourceAccountId
            }
          ]
        }
      ])
    })
  );
  await page.route('**/api/pixiv/account', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        account_id: bookmarkWriteback
          ? '0198f64c-42a2-7374-bace-9f1c3b317fb6'
          : null,
        pixiv_user_id: bookmarkWriteback ? 2002 : null,
        display_name: bookmarkWriteback ? 'Test Account' : null,
        state: bookmarkWriteback ? 'normal' : 'unconfigured',
        bookmark_writeback_enabled: bookmarkWriteback,
        last_validated_at: bookmarkWriteback ? '2026-07-30T12:00:00Z' : null,
        revision: bookmarkWriteback ? 1 : null
      })
    })
  );
  await page.route('**/api/system/settings', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        value: {
          storage: { trash_retention_days: 30 },
          pixiv: { default_private_bookmark: false },
          ugoira: {
            max_zip_bytes: 134217728,
            max_frames: 2000,
            max_pixels_per_frame: 40000000,
            decoded_frame_cache_bytes: 201326592
          },
          content: {
            overview_allow_nsfw: false,
            mask_non_all_age_thumbnails: maskNonAllAgeThumbnails
          }
        }
      })
    })
  );
  await page.route('**/api/derivatives/*', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'image/svg+xml',
      body: '<svg xmlns="http://www.w3.org/2000/svg" width="800" height="1200"><rect width="800" height="1200" fill="#d95555"/></svg>'
    })
  );
  await page.route('**/api/media/*/source', (route) => {
    if (kind === 'ugoira') {
      return route.fulfill({
        status: 200,
        contentType: 'application/zip',
        body: ugoiraZip()
      });
    }
    const landscape = route.request().url().includes(SECOND_MEDIA_ID);
    return route.fulfill({
      status: 200,
      contentType: 'image/svg+xml',
      body: landscape
        ? '<svg xmlns="http://www.w3.org/2000/svg" width="1200" height="800"><rect width="1200" height="800" fill="#42a86b"/></svg>'
        : '<svg xmlns="http://www.w3.org/2000/svg" width="800" height="1200"><rect width="800" height="1200" fill="#2589d9"/></svg>'
    });
  });
  return {
    setCollectionState(value) {
      currentCollectionState = value;
    }
  };
}

function workSummary(
  kind: 'illustration' | 'ugoira',
  description = '作品描述',
  ageRating: 'all_age' | 'r18' | 'r18g' | 'unknown' = 'all_age',
  collectionState: 'collected' | 'metadata_only' | 'trash' = 'collected'
) {
  return {
    id: WORK_ID,
    pixiv_work_id: 1002,
    title: '蓝色花束',
    description,
    artist_id: '0198f64c-42a2-7374-bace-9f1c3b317fb5',
    pixiv_artist_id: 2002,
    artist_name: 'Sample Artist',
    series_id: null,
    series_title: null,
    work_kind: kind,
    age_rating: ageRating,
    ai_generated: false,
    page_count: kind === 'illustration' ? 2 : 1,
    collection_state: collectionState,
    source_state: 'present',
    bookmarked_by_current_account: false,
    bookmark_id: null,
    bookmark_count: 3200,
    view_count: 41000,
    like_count: 5500,
    comment_count: 18,
    pixiv_published_at: '2026-07-01T12:00:00Z',
    pixiv_updated_at: '2026-07-01T12:00:00Z',
    local_updated_at: '2026-07-30T12:00:00Z',
    cover_available: true,
    cover_url: `/api/derivatives/${COVER_ID}`,
    cover_width: 800,
    cover_height: 1200,
    media_kind: kind === 'ugoira' ? 'ugoira_zip' : 'source_image',
    tags: []
  };
}

async function dominantColor(
  locator: import('@playwright/test').Locator,
  property: string
): Promise<string> {
  return locator.evaluate(
    (element, name) => getComputedStyle(element).getPropertyValue(name).trim(),
    property
  );
}

function ugoiraZip(): Buffer {
  const frame = Buffer.from(
    'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=',
    'base64'
  );
  return Buffer.from(
    zipSync({
      '000000.png': frame,
      '000001.png': frame
    })
  );
}
