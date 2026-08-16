import { expect, test, type Locator, type Page } from '@playwright/test';

import {
  fulfillJson,
  mockApi,
  mockEffectiveSettings,
  mockPixivAccount
} from './support';

const globalSearchPaths = new Set([
  '/api/gallery/search',
  '/api/gallery/artists',
  '/api/gallery/tags',
  '/api/gallery/series'
]);

test('shell exposes overview status, navigation and the command palette', async ({
  page
}) => {
  await mockApi(page);
  const archiveRequests: string[] = [];
  page.on('request', (request) => {
    const pathname = new URL(request.url()).pathname;
    if (globalSearchPaths.has(pathname)) archiveRequests.push(pathname);
  });
  await page.goto('/overview');

  await expect(
    page.getByRole('heading', { name: '概览', exact: true })
  ).toBeVisible();
  await expect(
    page.getByRole('heading', { name: '任务队列', exact: true })
  ).toBeVisible();
  await expect(page.getByText('媒体存储空间不足')).toBeVisible();
  await expect(
    page
      .locator('.overview-metrics > div', { hasText: '媒体目录' })
      .getByText('空间不足', { exact: true })
  ).toHaveText('空间不足');
  await expect(
    page
      .locator('.service-list > div', { hasText: '媒体目录' })
      .locator('small')
  ).toHaveText('空间不足');
  await expect(page.getByText('Web服务', { exact: true })).toHaveCount(0);
  await expect(
    page.getByRole('navigation', { name: '主要导航' })
  ).toContainText('图库');
  await expect(
    page.getByRole('navigation', { name: '主要导航' })
  ).toContainText('规则');

  await page.keyboard.press('Control+K');
  const search = page.getByRole('dialog', { name: '全局搜索' });
  await expect(search).toBeVisible();
  await expect(
    page.getByPlaceholder('搜索作品、作者、标签、系列或页面')
  ).toBeFocused();
  await expect(search.getByText('系统概况', { exact: true })).toBeVisible();
  await expect(search.locator('[data-search-result-kind="page"]')).toHaveCount(
    6
  );
  const hasOpeningAnimation = await search.evaluate((element) =>
    element.getAnimations().some((animation) => {
      const duration = animation.effect?.getTiming().duration;
      return typeof duration === 'number' && duration > 0;
    })
  );
  expect(hasOpeningAnimation).toBe(true);
  expect(archiveRequests).toEqual([]);
});

test('global search shortcut preserves an active query', async ({ page }) => {
  await mockApi(page);
  await mockGlobalSearch(page);
  await page.goto('/overview');
  await expect(page.getByRole('button', { name: '全局搜索' })).toBeVisible();

  await page.keyboard.press('Control+K');
  const input = page.getByPlaceholder('搜索作品、作者、标签、系列或页面');
  await input.fill('夜空');
  await page.keyboard.press('Control+K');

  await expect(input).toBeFocused();
  await expect(input).toHaveValue('夜空');
});

test('global search presents all result kinds and opens the keyboard selection', async ({
  page
}) => {
  await mockApi(page);
  const requests = await mockGlobalSearch(page);
  await page.goto('/overview');
  await page.locator('.search-trigger').click();

  const search = page.getByRole('dialog', { name: '全局搜索' });
  const results = search.locator('.results');
  const initialResultsHeight = await results.evaluate(
    (element) => element.getBoundingClientRect().height
  );
  await search
    .getByPlaceholder('搜索作品、作者、标签、系列或页面')
    .fill('收藏');

  for (const kind of ['work', 'artist', 'tag', 'series', 'page']) {
    await expect(
      search.locator(`[data-search-result-kind="${kind}"]`)
    ).toHaveCount(1);
  }
  expect(requests.sort()).toEqual(['artist', 'series', 'tag', 'work']);
  const artistAvatar = search.getByRole('img', {
    name: '收藏家みつき头像'
  });
  await expect(artistAvatar).toHaveAttribute(
    'src',
    '/api/following/authors/3249187/avatar'
  );
  await expect(
    search.locator('[data-search-result-kind="artist"] .account-avatar')
  ).toHaveCSS('border-radius', '50%');
  await expect(search.locator('[aria-selected="true"]')).toContainText('收藏');
  await expect
    .poll(() =>
      results.evaluate((element) => element.getBoundingClientRect().height)
    )
    .toBeCloseTo(initialResultsHeight, 2);

  await page.keyboard.press('ArrowDown');
  await expect(search.locator('[aria-selected="true"]')).toContainText(
    '收藏中的夜空'
  );
  await page.keyboard.press('Enter');
  await expect(page).toHaveURL('/gallery/works/120001');
});

test('global search ignores a slower response from an earlier query', async ({
  page
}) => {
  await mockApi(page);
  await mockGlobalSearch(page);
  let releaseOldResponse!: () => void;
  const oldResponseGate = new Promise<void>((resolve) => {
    releaseOldResponse = resolve;
  });
  let reportOldResponse!: () => void;
  const oldResponseSent = new Promise<void>((resolve) => {
    reportOldResponse = resolve;
  });
  await page.route('**/api/gallery/search', async (route) => {
    const body = route.request().postDataJSON() as {
      groups?: Array<{ filters?: Array<{ value?: string }> }>;
    };
    const query = body.groups?.[0]?.filters?.[0]?.value;
    if (query === '旧查询') await oldResponseGate;
    await fulfillJson(route, 200, {
      items: [
        globalSearchWork(
          query === '旧查询' ? 120002 : 120003,
          query === '旧查询' ? '旧搜索结果' : '新搜索结果'
        )
      ],
      next_cursor: null
    });
    if (query === '旧查询') reportOldResponse();
  });

  await page.goto('/overview');
  await page.locator('.search-trigger').click();
  const search = page.getByRole('dialog', { name: '全局搜索' });
  const input = search.getByPlaceholder('搜索作品、作者、标签、系列或页面');
  const oldRequest = page.waitForRequest(
    (request) =>
      new URL(request.url()).pathname === '/api/gallery/search' &&
      request.postData()?.includes('旧查询') === true
  );
  await input.fill('旧查询');
  await oldRequest;

  await input.fill('新查询');
  await expect(search.getByText('新搜索结果', { exact: true })).toBeVisible();
  releaseOldResponse();
  await oldResponseSent;
  await page.evaluate(
    () => new Promise<void>((resolve) => requestAnimationFrame(() => resolve()))
  );
  await expect(search.getByText('旧搜索结果', { exact: true })).toHaveCount(0);
  await expect(search.getByText('新搜索结果', { exact: true })).toBeVisible();
});

test('global search keeps partial results usable on a narrow screen and resets after closing', async ({
  page
}) => {
  await mockApi(page);
  await mockGlobalSearch(page, 'tag');
  await page.setViewportSize({ width: 320, height: 720 });
  await page.goto('/overview');
  await page.getByRole('button', { name: '全局搜索' }).click();

  const search = page.getByRole('dialog', { name: '全局搜索' });
  const input = search.getByPlaceholder('搜索作品、作者、标签、系列或页面');
  await input.fill('收藏');
  await expect(search.getByText('标签暂时无法读取')).toBeVisible();
  await expect(search.getByText('收藏中的夜空', { exact: true })).toBeVisible();

  const dialogBox = await search.boundingBox();
  expect(dialogBox).not.toBeNull();
  expect(dialogBox!.x).toBeGreaterThanOrEqual(0);
  expect(dialogBox!.x + dialogBox!.width).toBeLessThanOrEqual(320);
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= window.innerWidth
    )
  ).toBe(true);

  await search.evaluate((element) => {
    element.style.animationPlayState = 'paused';
  });
  await page.keyboard.press('Escape');
  await expect(search).toHaveClass(/closing/);
  const closeAnimationDuration = await search.evaluate((element) =>
    element.getAnimations().reduce((duration, animation) => {
      const current = animation.effect?.getTiming().duration;
      return typeof current === 'number'
        ? Math.max(duration, current)
        : duration;
    }, 0)
  );
  expect(closeAnimationDuration).toBeGreaterThan(0);
  await search.evaluate((element) => {
    element.style.animationPlayState = 'running';
  });
  await expect(search).toBeHidden();
  await page.locator('.search-trigger').click();
  await expect(input).toHaveValue('');
  await expect(search.locator('[data-search-result-kind="page"]')).toHaveCount(
    6
  );
  await expect(search.locator('[data-search-result-kind="work"]')).toHaveCount(
    0
  );
});

test('overview requests the browser-local day and reuses the server selection', async ({
  page
}) => {
  await mockApi(page);
  await page.unroute('**/api/gallery/overview-decorations**');
  const requests: Array<{ method: string; date: string | null }> = [];
  await page.route('**/api/gallery/overview-decorations**', async (route) => {
    const request = route.request();
    requests.push({
      method: request.method(),
      date: new URL(request.url()).searchParams.get('date')
    });
    await fulfillJson(route, 200, {
      items: [
        {
          pixiv_work_id: 7101,
          title: '装饰图一',
          age_rating: 'all_age',
          cover_url: '/overview-cover-1'
        },
        {
          pixiv_work_id: 7102,
          title: '装饰图二',
          age_rating: 'all_age',
          cover_url: '/overview-cover-2'
        },
        {
          pixiv_work_id: 7101,
          title: '装饰图一',
          age_rating: 'all_age',
          cover_url: '/overview-cover-1'
        }
      ]
    });
  });
  await page.route('**/overview-cover-*', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'image/svg+xml',
      body: '<svg xmlns="http://www.w3.org/2000/svg" width="320" height="180"><rect width="320" height="180" fill="#0096fa"/></svg>'
    })
  );

  await page.goto('/overview');
  await expect
    .poll(() =>
      page
        .locator('.quick-links a')
        .evaluateAll((links) =>
          links.map((link) => link.getAttribute('data-decoration-work-id'))
        )
    )
    .toEqual(['7101', '7102', '7101']);
  const localDate = await page.evaluate(() => {
    const now = new Date();
    const year = now.getFullYear();
    const month = String(now.getMonth() + 1).padStart(2, '0');
    const day = String(now.getDate()).padStart(2, '0');
    return `${year}-${month}-${day}`;
  });
  expect(requests).toEqual([{ method: 'GET', date: localDate }]);

  await page.reload();
  await expect
    .poll(() =>
      page
        .locator('.quick-links a')
        .evaluateAll((links) =>
          links.map((link) => link.getAttribute('data-decoration-work-id'))
        )
    )
    .toEqual(['7101', '7102', '7101']);
  expect(requests).toEqual([
    { method: 'GET', date: localDate },
    { method: 'GET', date: localDate }
  ]);
});

test('overview masks restricted selections without classifying empty slots', async ({
  page
}) => {
  await mockApi(page);
  await page.unroute('**/api/system/settings');
  await page.unroute('**/api/gallery/overview-decorations**');
  await page.route('**/api/system/settings', (route) =>
    fulfillJson(route, 200, {
      value: mockEffectiveSettings({ mask_non_all_age_thumbnails: true })
    })
  );
  await page.route('**/api/gallery/overview-decorations**', (route) =>
    fulfillJson(route, 200, {
      items: [
        {
          pixiv_work_id: 7102,
          title: '受限装饰图',
          age_rating: 'r18',
          cover_url: '/overview-restricted-cover'
        },
        null,
        null
      ]
    })
  );
  let coverRequests = 0;
  await page.route('**/overview-restricted-cover', (route) => {
    coverRequests += 1;
    return route.fulfill({ status: 200, body: 'unexpected image request' });
  });

  await page.goto('/overview');

  const shortcut = page.locator('[data-decoration-work-id="7102"]');
  await expect(shortcut.getByText('缩略图已遮挡')).toBeVisible();
  const emptyShortcut = page.locator('.quick-links a').nth(1);
  await expect(emptyShortcut.getByText('暂无可用装饰图')).toBeVisible();
  await expect(emptyShortcut.getByText('缩略图已遮挡')).toHaveCount(0);
  expect(coverRequests).toBe(0);
});

test('overview reports a decoration read failure and retries it', async ({
  page
}) => {
  await mockApi(page);
  await page.unroute('**/api/gallery/overview-decorations**');
  let requests = 0;
  await page.route('**/api/gallery/overview-decorations**', async (route) => {
    requests += 1;
    if (requests === 1) {
      await fulfillJson(route, 503, {
        error: {
          code: 'service_unavailable',
          message: 'overview decorations are temporarily unavailable'
        }
      });
      return;
    }
    await fulfillJson(route, 200, { items: [null, null, null] });
  });

  await page.goto('/overview');

  await expect(page.getByText('概览装饰图暂时无法读取')).toBeVisible();
  await page.getByRole('button', { name: '重新读取概览装饰图' }).click();
  await expect(page.getByText('概览装饰图暂时无法读取')).toHaveCount(0);
  expect(requests).toBe(2);
});

test('overview keeps its natural proportions, aligned shortcuts and clean lower edge', async ({
  page
}) => {
  await mockApi(page);
  await page.setViewportSize({ width: 1280, height: 720 });
  await page.goto('/overview');
  await expect(page.getByText('媒体存储空间不足')).toBeVisible();

  const queueBoard = page.locator('.queue-board');
  const serviceBoard = page.locator('.service-board');
  const shortcuts = page.locator('.quick-links a');
  const quickBoard = page.locator('.quick-board');
  const main = page.locator('main.overview-main');
  const [
    queueBox,
    serviceBox,
    firstShortcutBox,
    lastShortcutBox,
    quickBox,
    mainBox
  ] = await Promise.all([
    queueBoard.boundingBox(),
    serviceBoard.boundingBox(),
    shortcuts.first().boundingBox(),
    shortcuts.last().boundingBox(),
    quickBoard.boundingBox(),
    main.boundingBox()
  ]);

  expect(queueBox).not.toBeNull();
  expect(serviceBox).not.toBeNull();
  expect(firstShortcutBox).not.toBeNull();
  expect(lastShortcutBox).not.toBeNull();
  expect(quickBox).not.toBeNull();
  expect(mainBox).not.toBeNull();
  const boardLayout = await page.evaluate(() => {
    const snapshot = (boardSelector: string, rowSelector: string) => {
      const board = document.querySelector(boardSelector);
      const rows = [...document.querySelectorAll(rowSelector)];
      if (!board || rows.length === 0) {
        throw new Error(`missing overview board content: ${boardSelector}`);
      }
      const boardBox = board.getBoundingClientRect();
      const lastRowBox = rows.at(-1)!.getBoundingClientRect();
      return {
        overflowY: getComputedStyle(board).overflowY,
        boardBottom: boardBox.bottom,
        lastRowBottom: lastRowBox.bottom
      };
    };
    const overview = document.querySelector('.overview');
    const railItem = document.querySelector('.overview-metrics > div');
    const queueRow = document.querySelector('.queue-row');
    const quickLink = document.querySelector('.quick-links a');
    if (!overview || !railItem || !queueRow || !quickLink) {
      throw new Error('overview layout is incomplete');
    }
    return {
      overview: {
        display: getComputedStyle(overview).display,
        rowGap: getComputedStyle(overview).rowGap
      },
      railMinHeight: getComputedStyle(railItem).minHeight,
      queueRowMinHeight: getComputedStyle(queueRow).minHeight,
      quickLinkMinHeight: getComputedStyle(quickLink).minHeight,
      queue: snapshot('.queue-board', '.queue-row'),
      services: snapshot('.service-board', '.service-list > div')
    };
  });
  expect(boardLayout.overview).toEqual({ display: 'grid', rowGap: '22px' });
  expect(boardLayout.railMinHeight).toBe('105px');
  expect(boardLayout.queueRowMinHeight).toBe('54px');
  expect(boardLayout.quickLinkMinHeight).toBe('112px');
  expect(boardLayout.queue.overflowY).not.toBe('hidden');
  expect(boardLayout.services.overflowY).not.toBe('hidden');
  expect(boardLayout.queue.lastRowBottom).toBeLessThanOrEqual(
    boardLayout.queue.boardBottom + 1
  );
  expect(boardLayout.services.lastRowBottom).toBeLessThanOrEqual(
    boardLayout.services.boardBottom + 1
  );
  const dividerStyles = await quickBoard.evaluate((element) => {
    const styles = getComputedStyle(element);
    return {
      top: {
        width: Number.parseFloat(styles.borderTopWidth),
        style: styles.borderTopStyle,
        color: styles.borderTopColor
      },
      bottom: {
        width: Number.parseFloat(styles.borderBottomWidth),
        style: styles.borderBottomStyle,
        color: styles.borderBottomColor
      }
    };
  });
  expect(dividerStyles.top.width).toBe(0);
  expect(dividerStyles.bottom.width).toBe(0);
  expect(Math.abs(firstShortcutBox!.x - queueBox!.x)).toBeLessThanOrEqual(1);
  expect(
    Math.abs(
      lastShortcutBox!.x +
        lastShortcutBox!.width -
        (serviceBox!.x + serviceBox!.width)
    )
  ).toBeLessThanOrEqual(1);
  expect(
    Math.abs(quickBox!.y + quickBox!.height - (mainBox!.y + mainBox!.height))
  ).toBeLessThanOrEqual(1);
  const alertBox = await page.locator('.alert-banner').boundingBox();
  expect(alertBox).not.toBeNull();
  expect(alertBox!.height).toBeLessThanOrEqual(86);
});

test('navigation pages share one heading treatment', async ({ page }) => {
  await mockApi(page);
  await page.route('**/api/gallery/search', (route) =>
    fulfillJson(route, 200, {
      items: [],
      next_cursor: null,
      total_count: 0
    })
  );
  const routes = [
    { path: '/overview', name: '概览' },
    { path: '/gallery', name: '图库' },
    { path: '/rules', name: '规则工作台' },
    { path: '/system/settings', name: '系统设置' }
  ];
  const headings: Array<{
    fontSize: string;
    fontWeight: string;
    lineHeight: string;
    letterSpacing: string;
    x: number;
  }> = [];

  for (const route of routes) {
    await page.goto(route.path);
    const heading = page.getByRole('heading', {
      name: route.name,
      exact: true
    });
    await expect(heading).toBeVisible();
    headings.push(
      await heading.evaluate((element) => {
        const styles = getComputedStyle(element);
        return {
          fontSize: styles.fontSize,
          fontWeight: styles.fontWeight,
          lineHeight: styles.lineHeight,
          letterSpacing: styles.letterSpacing,
          x: element.getBoundingClientRect().x
        };
      })
    );
  }

  for (const heading of headings.slice(1)) {
    expect(heading).toEqual(headings[0]);
  }
});

test('overview shortcuts use a lighter lower surface and shared card border', async ({
  page
}) => {
  await mockApi(page);
  await page.goto('/overview');

  const shortcut = page.locator('.quick-links a').first();
  const treatment = await shortcut.evaluate((element) => {
    const shade = element.querySelector('.quick-shade');
    const label = element.querySelector('strong');
    if (!shade || !label) throw new Error('overview shortcut is incomplete');
    const elementBox = element.getBoundingClientRect();
    const labelBox = label.getBoundingClientRect();
    const styles = getComputedStyle(element);
    return {
      borderWidth: styles.borderTopWidth,
      borderStyle: styles.borderTopStyle,
      borderColor: styles.borderTopColor,
      backgroundImage: getComputedStyle(shade).backgroundImage,
      labelBottomInset: elementBox.bottom - labelBox.bottom
    };
  });

  expect(treatment.borderWidth).toBe('1px');
  expect(treatment.borderStyle).toBe('solid');
  expect(treatment.borderColor).not.toBe('rgba(0, 0, 0, 0)');
  expect(treatment.backgroundImage).toMatch(/(?:70%|\/ 0\.7(?:0+)?\))/);
  expect(treatment.labelBottomInset).toBeGreaterThanOrEqual(22);
});

test('shell brand reuses the browser icon asset', async ({ page }) => {
  await mockApi(page);
  await page.goto('/overview');

  const brandIcon = page
    .getByRole('link', { name: 'PixivArchive概览' })
    .locator('img');
  await expect(brandIcon).toHaveAttribute('src', '/favicon.svg');
  await expect(brandIcon).toHaveAttribute('alt', '');
  await expect
    .poll(() => brandIcon.evaluate((image: HTMLImageElement) => image.complete))
    .toBe(true);
  await expect
    .poll(() =>
      brandIcon.evaluate((image: HTMLImageElement) => image.naturalWidth)
    )
    .toBeGreaterThan(0);
  const browserIcon = page.locator('link[rel="icon"]');
  await expect(browserIcon).toHaveAttribute('href', /\/favicon\.svg$/);
  await expect
    .poll(() =>
      Promise.all([
        browserIcon.evaluate((link: HTMLLinkElement) => link.href),
        brandIcon.evaluate((image: HTMLImageElement) => image.currentSrc)
      ])
    )
    .toEqual([
      new URL('/favicon.svg', page.url()).href,
      new URL('/favicon.svg', page.url()).href
    ]);
});

test('shell brand keeps the white folder distinct from the white P', async ({
  page
}) => {
  await mockApi(page);
  await page.goto('/overview');

  const brandIcon = page
    .getByRole('link', { name: 'PixivArchive概览' })
    .locator('img');
  const colors = await brandIcon.evaluate(async (image: HTMLImageElement) => {
    await image.decode();
    const canvas = document.createElement('canvas');
    canvas.width = 64;
    canvas.height = 64;
    const context = canvas.getContext('2d');
    if (!context) throw new Error('Canvas 2D context is unavailable');
    context.drawImage(image, 0, 0, canvas.width, canvas.height);

    const sample = (x: number, y: number) =>
      Array.from(context.getImageData(x, y, 1, 1).data.slice(0, 3));
    return {
      folderInterior: sample(45, 51),
      folderBoundary: sample(34, 40)
    };
  });

  expect(colors.folderInterior).toEqual([255, 255, 255]);
  expect(colors.folderBoundary).toEqual([0, 150, 250]);
});

test('top bar menus share one readable surface without overlapping theme options', async ({
  page
}) => {
  await mockApi(page);
  await page.goto('/overview');

  await page.getByRole('button', { name: '主题' }).click();
  const themePopover = page.locator('[data-topbar-popover="theme"]');
  const themeStyle = await readPopoverStyle(themePopover);

  await page.getByRole('button', { name: '管理员菜单' }).click();
  const accountPopover = page.locator('[data-topbar-popover="account"]');
  const accountStyle = await readPopoverStyle(accountPopover);

  expect(themeStyle).toEqual(accountStyle);
  expect(backgroundAlpha(themeStyle.backgroundColor)).toBeGreaterThanOrEqual(
    0.93
  );

  await page.setViewportSize({ width: 390, height: 844 });
  await page.getByRole('button', { name: '打开导航' }).click();
  const navigationPopover = page.locator('[data-topbar-popover="navigation"]');
  const navigationStyle = await readPopoverStyle(navigationPopover);
  expect(navigationStyle).toEqual(accountStyle);
  await expect(navigationPopover).toHaveCSS('position', 'fixed');
  await page.getByRole('button', { name: '打开导航' }).click();

  await page.getByRole('button', { name: '管理员菜单' }).click();
  const narrowAccountPopover = page.locator('[data-topbar-popover="account"]');
  const accountBox = await narrowAccountPopover.boundingBox();
  await page.getByRole('button', { name: '主题' }).click();
  const narrowThemePopover = page.locator('[data-topbar-popover="theme"]');
  const themeBox = await narrowThemePopover.boundingBox();

  expect(accountBox).not.toBeNull();
  expect(themeBox).not.toBeNull();
  expect(accountBox!.x).toBe(12);
  expect(themeBox!.x).toBe(12);
  expect(accountBox!.width).toBe(themeBox!.width);
  expect(accountBox!.width).toBe(366);
  await expect(narrowThemePopover).toHaveCSS('position', 'fixed');

  const optionBoxes = await narrowThemePopover
    .locator('.popover-item')
    .evaluateAll((items) => items.map((item) => item.getBoundingClientRect()));
  for (let index = 1; index < optionBoxes.length; index += 1) {
    expect(optionBoxes[index].top).toBeGreaterThanOrEqual(
      optionBoxes[index - 1].bottom
    );
  }
});

test('shared metric strips keep complete narrow-screen dividers', async ({
  page
}) => {
  await mockApi(page);
  await page.setViewportSize({ width: 760, height: 900 });

  await page.goto('/overview');
  const overviewCells = page.locator('.overview-metrics > div');
  await expect(overviewCells).toHaveCount(4);
  await expect(overviewCells.nth(0)).toHaveCSS('border-bottom-width', '1px');
  await expect(overviewCells.nth(1)).toHaveCSS('border-bottom-width', '1px');
  await expect(overviewCells.nth(1)).toHaveCSS('border-right-width', '0px');

  await page.goto('/system/trash');
  const trashCells = page.locator('.trash-metrics > div');
  await expect(trashCells).toHaveCount(3);
  const lastBox = await trashCells.last().boundingBox();
  const stripBox = await page.locator('.trash-metrics').boundingBox();
  expect(lastBox).not.toBeNull();
  expect(stripBox).not.toBeNull();
  expect(Math.abs(lastBox!.width - (stripBox!.width - 2))).toBeLessThanOrEqual(
    1
  );
  await expect(trashCells.last()).toHaveCSS('border-top-width', '1px');
  await expect(trashCells.last()).toHaveCSS('border-right-width', '0px');
});

test('overview keeps its confirmed queue snapshot until a refresh succeeds', async ({
  page
}) => {
  await mockApi(page);
  await page.unroute('**/api/system/status');
  await page.unroute('**/api/events');

  let releaseInitialStatus!: () => void;
  const initialStatusReady = new Promise<void>((resolve) => {
    releaseInitialStatus = resolve;
  });
  let releaseFailedRefresh!: () => void;
  const failedRefreshReady = new Promise<void>((resolve) => {
    releaseFailedRefresh = resolve;
  });
  let reportRefreshStarted!: () => void;
  const refreshStarted = new Promise<void>((resolve) => {
    reportRefreshStarted = resolve;
  });
  let publishJobEvent!: () => void;
  const jobEvent = new Promise<void>((resolve) => {
    publishJobEvent = resolve;
  });
  let eventConnections = 0;
  let statusRequests = 0;

  await page.route('**/api/system/status', async (route) => {
    statusRequests += 1;
    if (statusRequests === 1) {
      await initialStatusReady;
      await fulfillJson(
        route,
        200,
        systemStatus({
          immediate: {
            queued: 2,
            waiting_account: 3,
            waiting_storage: 4,
            running: 1
          },
          manual_import: { queued: 1, running: 0 }
        })
      );
      return;
    }
    if (statusRequests === 2) {
      reportRefreshStarted();
      await failedRefreshReady;
      await fulfillJson(route, 503, {
        error: {
          code: 'service_unavailable',
          message: 'system status is temporarily unavailable'
        }
      });
      return;
    }
    await fulfillJson(route, 200, systemStatus({}));
  });
  await page.route('**/api/events', async (route) => {
    eventConnections += 1;
    if (eventConnections > 1) {
      await route.fulfill({ status: 204 });
      return;
    }
    await jobEvent;
    await route.fulfill({
      status: 200,
      headers: { 'Content-Type': 'text/event-stream' },
      body: 'event: app_event\ndata: {"resource":"job","resource_id":"0198f64c-42a2-7374-bace-9f1c3b317fa3"}\n\n'
    });
  });

  await page.goto('/overview');
  const waitingMetric = page.locator('.overview-metrics > div', {
    hasText: '队列等待'
  });
  const runningMetric = page.locator('.overview-metrics > div', {
    hasText: '正在处理'
  });
  await expect(waitingMetric.locator('strong')).toHaveText('—');
  await expect(runningMetric.locator('strong')).toHaveText('—');

  releaseInitialStatus();
  await expect(waitingMetric.locator('strong')).toHaveText('10');
  await expect(runningMetric.locator('strong')).toHaveText('1');

  publishJobEvent();
  await refreshStarted;
  await expect(waitingMetric.locator('strong')).toHaveText('10');
  await expect(runningMetric.locator('strong')).toHaveText('1');

  releaseFailedRefresh();
  await expect(
    page.getByText('系统状态读取失败', { exact: true })
  ).toBeVisible();
  await expect(page.getByText('请稍后重试', { exact: true })).toBeVisible();
  await expect(waitingMetric.locator('strong')).toHaveText('10');
  await expect(runningMetric.locator('strong')).toHaveText('1');

  await page.getByRole('button', { name: '重新读取' }).click();
  await expect(waitingMetric.locator('strong')).toHaveText('0');
  await expect(runningMetric.locator('strong')).toHaveText('0');
  await expect(page.getByText('系统状态读取失败', { exact: true })).toHaveCount(
    0
  );
  expect(statusRequests).toBe(3);
});

test('overview explains how an invalid Pixiv account affects queued work', async ({
  page
}) => {
  await mockApi(page);
  await page.setViewportSize({ width: 1280, height: 720 });
  await page.unroute('**/api/pixiv/account');
  await page.unroute('**/api/system/status');
  await page.route('**/api/pixiv/account', async (route) => {
    await fulfillJson(
      route,
      200,
      mockPixivAccount({
        account_id: '0198f64c-42a2-7374-bace-9f1c3b317fa1',
        pixiv_user_id: 10001,
        display_name: 'Account A',
        avatar_url: 'https://i.pximg.net/avatar-a.svg',
        state: 'credential_invalid'
      })
    );
  });
  await page.route('**/api/system/status', async (route) => {
    await fulfillJson(
      route,
      200,
      systemStatus({
        scheduled_collection: { waiting_account: 4, running: 0 }
      })
    );
  });

  await page.goto('/overview');

  await expect(page.getByText('Pixiv账户需要重新验证')).toBeVisible();
  await expect(
    page.getByText(
      'Cookie已经失效，依赖该账户的订阅和任务会保持等待。更新并验证Cookie后会自动继续。'
    )
  ).toBeVisible();
  await expect(
    page
      .locator('.overview-metrics > div', { hasText: '队列等待' })
      .locator('strong')
  ).toHaveText('4');
  await page.evaluate(() => window.scrollTo(0, document.body.scrollHeight));
  await expect(page.locator('.quick-board')).toBeInViewport();

  const banner = page.locator('.alert-banner');
  const centers = await banner.evaluate((element) => {
    const icon = element.querySelector('.icon');
    const copy = element.querySelector(':scope > div');
    if (!icon || !copy) throw new Error('alert banner content is incomplete');
    const iconBox = icon.getBoundingClientRect();
    const copyBox = copy.getBoundingClientRect();
    return {
      icon: iconBox.top + iconBox.height / 2,
      copy: copyBox.top + copyBox.height / 2
    };
  });
  expect(Math.abs(centers.icon - centers.copy)).toBeLessThanOrEqual(1);
});

test('overview keeps natural document scrolling in a low desktop viewport', async ({
  page
}) => {
  await mockApi(page);
  await page.setViewportSize({ width: 1280, height: 640 });
  await page.goto('/overview');

  await expect
    .poll(() =>
      page.evaluate(
        () =>
          document.documentElement.scrollHeight -
          document.documentElement.clientHeight
      )
    )
    .toBeGreaterThan(1);
  await page.evaluate(() => window.scrollTo(0, document.body.scrollHeight));
  await expect(page.locator('.quick-board')).toBeInViewport();
  await expect(page.locator('.quick-board')).toHaveCSS(
    'border-bottom-width',
    '0px'
  );
});

test('narrow screens use the compact navigation', async ({ page }) => {
  await mockApi(page);
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto('/overview');

  await expect(page.getByRole('button', { name: '打开导航' })).toBeVisible();
  await expect(page.getByRole('navigation', { name: '主要导航' })).toBeHidden();
});

test('secondary navigation only exposes current product sections', async ({
  page
}) => {
  await mockApi(page);
  await page.route('**/api/gallery/search', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({ items: [], next_cursor: null })
    })
  );
  await page.goto('/gallery');

  const navigation = page.getByRole('navigation', { name: '二级导航' });
  await expect(navigation).toContainText('作者');
  await expect(navigation).toContainText('标签');
  await expect(navigation).toContainText('系列');
  await expect(navigation).not.toContainText('标签与系列');
  await expect(navigation).not.toContainText('仅有元数据');
  await expect(navigation).not.toContainText('重复文件');
  await expect(navigation).not.toContainText('隐藏作品');
  await expect(navigation).not.toContainText('相似图片');
  await expect(navigation.getByRole('link', { name: '返回' })).toHaveCount(0);
});

test('discovery navigation names subscriptions as plans', async ({ page }) => {
  await mockApi(page);
  await page.goto('/discovery/subscriptions');

  const navigation = page.getByRole('navigation', { name: '二级导航' });
  await expect(navigation).toContainText('订阅计划');
  await expect(page.getByRole('heading', { name: '订阅计划' })).toBeVisible();
});

test('system navigation enters the first system page', async ({ page }) => {
  await mockApi(page);
  await page.goto('/overview');

  await page
    .getByRole('navigation', { name: '主要导航' })
    .getByText('系统')
    .click();
  await expect(page).toHaveURL('/system/account');
});

test('administrator avatar follows the Pixiv identity selected by a cookie update', async ({
  page
}) => {
  await mockApi(page);
  await page.unroute('**/api/events');

  const accountA = mockPixivAccount({
    account_id: '0198f64c-42a2-7374-bace-9f1c3b317fa1',
    pixiv_user_id: 10001,
    display_name: 'Account A',
    avatar_url: 'https://i.pximg.net/avatar-a.svg',
    last_validated_at: '2026-08-08T00:00:00Z'
  });
  const accountB = mockPixivAccount({
    account_id: '0198f64c-42a2-7374-bace-9f1c3b317fa2',
    pixiv_user_id: 10002,
    display_name: 'Account B',
    avatar_url: 'https://i.pximg.net/avatar-b.svg',
    last_validated_at: '2026-08-08T00:00:00Z'
  });
  let currentAccount = accountA;
  let publishAccountChange!: () => void;
  const accountChanged = new Promise<void>((resolve) => {
    publishAccountChange = resolve;
  });
  let eventConnections = 0;

  await page.route('**/api/events', async (route) => {
    eventConnections += 1;
    if (eventConnections > 1) {
      await route.fulfill({ status: 204 });
      return;
    }
    await accountChanged;
    await route.fulfill({
      status: 200,
      headers: { 'Content-Type': 'text/event-stream' },
      body: `event: app_event\ndata: {"resource":"pixiv_account","resource_id":"${accountB.account_id}"}\n\n`
    });
  });
  await page.route('**/api/pixiv/account', async (route) => {
    if (route.request().method() === 'PUT') {
      currentAccount = accountB;
      publishAccountChange();
    }
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify(currentAccount)
    });
  });
  await page.route('https://i.pximg.net/**', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'image/svg+xml',
      body: '<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32"><rect width="32" height="32" fill="#209cee"/></svg>'
    })
  );

  await page.goto('/system/account');
  const avatar = page.locator('.avatar-button img');
  await expect(avatar).toHaveAttribute('src', accountA.avatar_url);

  await page.getByPlaceholder('PHPSESSID或完整Cookie').fill('cookie-b');
  await page.getByRole('button', { name: '保存并验证' }).click();

  await expect(page.getByText('验证成功')).toBeVisible();
  await expect(avatar).toHaveAttribute('src', accountB.avatar_url);
});

async function readPopoverStyle(locator: Locator) {
  return locator.evaluate((element) => {
    const style = getComputedStyle(element);
    return {
      backgroundColor: style.backgroundColor,
      borderTop: `${style.borderTopWidth} ${style.borderTopStyle} ${style.borderTopColor}`,
      boxShadow: style.boxShadow,
      borderRadius: style.borderRadius,
      padding: style.padding
    };
  });
}

async function mockGlobalSearch(
  page: Page,
  failedGroup?: 'work' | 'artist' | 'tag' | 'series'
): Promise<string[]> {
  const requests: string[] = [];
  await page.route('**/api/gallery/search', async (route) => {
    requests.push('work');
    if (failedGroup === 'work') {
      await fulfillJson(route, 503, { error: 'work search unavailable' });
      return;
    }
    await fulfillJson(route, 200, {
      items: [globalSearchWork(120001, '收藏中的夜空')],
      next_cursor: null
    });
  });
  await page.route('**/api/gallery/artists?*', async (route) => {
    requests.push('artist');
    if (failedGroup === 'artist') {
      await fulfillJson(route, 503, { error: 'artist search unavailable' });
      return;
    }
    await fulfillJson(route, 200, {
      items: [
        {
          id: '0198f64c-42a2-7374-bace-9f1c3b317fa2',
          pixiv_artist_id: 3249187,
          name: '收藏家みつき',
          work_count: 128,
          cover_url: '/global-search-artist.svg',
          cover_age_rating: 'all_age'
        }
      ],
      next_cursor: null,
      total: 1
    });
  });
  await page.route('**/api/gallery/tags?*', async (route) => {
    requests.push('tag');
    if (failedGroup === 'tag') {
      await fulfillJson(route, 503, { error: 'tag search unavailable' });
      return;
    }
    await fulfillJson(route, 200, {
      items: [
        {
          tag: {
            id: '0198f64c-42a2-7374-bace-9f1c3b317fa3',
            original: 'bookmark',
            translation: '收藏'
          },
          work_count: 9
        }
      ],
      next_cursor: null,
      total: 1
    });
  });
  await page.route('**/api/gallery/series?*', async (route) => {
    requests.push('series');
    if (failedGroup === 'series') {
      await fulfillJson(route, 503, { error: 'series search unavailable' });
      return;
    }
    await fulfillJson(route, 200, {
      items: [
        {
          id: '0198f64c-42a2-7374-bace-9f1c3b317fa4',
          pixiv_series_id: 923804,
          title: '收藏的星屑观测记',
          work_count: 12,
          cover_url: '/global-search-series.svg',
          cover_age_rating: 'all_age'
        }
      ],
      next_cursor: null,
      total: 1
    });
  });
  await page.route('**/global-search-*.svg', async (route) => {
    await route.fulfill({
      status: 200,
      contentType: 'image/svg+xml',
      body: '<svg xmlns="http://www.w3.org/2000/svg" width="96" height="96"><rect width="96" height="96" fill="#0096fa"/><circle cx="68" cy="28" r="16" fill="#fff" opacity=".9"/></svg>'
    });
  });
  await page.route('**/api/following/authors/3249187/avatar', async (route) => {
    await route.fulfill({
      status: 200,
      contentType: 'image/svg+xml',
      body: '<svg xmlns="http://www.w3.org/2000/svg" width="96" height="96"><circle cx="48" cy="48" r="48" fill="#0096fa"/></svg>'
    });
  });
  return requests;
}

function globalSearchWork(pixivWorkId: number, title: string) {
  return {
    id: `0198f64c-42a2-7374-bace-${pixivWorkId}`,
    pixiv_work_id: pixivWorkId,
    title,
    artist_name: '夜空みつき',
    age_rating: 'all_age',
    cover_url: '/global-search-work.svg'
  };
}

function backgroundAlpha(color: string): number {
  const match = color.match(/rgba?\([^,]+,[^,]+,[^,]+(?:,\s*([\d.]+))?\)/);
  return match?.[1] ? Number(match[1]) : 1;
}

function systemStatus(queue: Record<string, Record<string, number>>) {
  return {
    version: '0.1.0',
    git_commit: 'daf3069',
    migration_version: 14,
    database: { status: 'healthy', message: null },
    worker: { status: 'healthy', message: null },
    media: { status: 'warning', message: '媒体盘剩余88 GiB' },
    queue,
    setting_revisions: {},
    storage: {
      available_bytes: 94_489_280_512,
      warning_threshold_bytes: 107_374_182_400,
      stop_threshold_bytes: 34_359_738_368,
      write_stopped: false
    },
    capabilities: {
      avif_derivatives: true,
      reflink: true,
      webp_derivatives: true
    }
  };
}
