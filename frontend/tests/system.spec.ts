import { expect, test, type Page } from '@playwright/test';

import { fulfillJson, mockApi } from './support';

const accountId = '0198f653-0000-7000-8000-000000000001';

test('Pixiv account validation keeps restricted and invalid states visible', async ({
  page
}) => {
  const api = await mockSystem(page);
  await page.goto('/system/account');

  await expect(page.getByRole('heading', { name: 'Pixiv账户' })).toBeVisible();
  await expect(page.locator('section[data-settings-card]')).toHaveCount(3);
  await expect(page.getByText('受限访问')).toBeVisible();
  await expect(page.getByText('10001')).toBeVisible();
  await expect(page.getByText('Test Artist')).toBeVisible();
  const accountMetricSizes = await page
    .locator('.account-metrics > div strong')
    .evaluateAll((values) =>
      values.map((value) => getComputedStyle(value).fontSize)
    );
  expect(accountMetricSizes).toEqual(['18.4px', '18.4px', '18.4px', '14.72px']);
  await page
    .getByLabel('Pixiv Cookie')
    .fill('PHPSESSID=secret-value-that-must-not-be-rendered-again');
  await page.getByRole('button', { name: '保存并验证' }).click();

  await expect(page.getByText('凭据失效')).toBeVisible();
  await expect(
    page.getByRole('button', { name: '清除Pixiv Cookie' })
  ).toBeVisible();
  expect(api.accountUpdate).toEqual({
    cookie: 'PHPSESSID=secret-value-that-must-not-be-rendered-again'
  });
  await expect(page.getByLabel('Pixiv Cookie')).toHaveValue('');
});

test('bookmark write-back centers its switch and feedback on the two-line copy', async ({
  page
}) => {
  const api = await mockSystem(page);
  await page.goto('/system/account');

  const toggle = page.getByLabel('同步修改Pixiv收藏');
  await expect(toggle).not.toBeChecked();
  await toggle.check();

  await expect
    .poll(() => api.bookmarkSetting)
    .toEqual({
      expected_account_id: accountId,
      enabled: true,
      expected_revision: 4
    });
  const card = page.locator('section[data-settings-card]').filter({
    hasText: '收藏回写'
  });
  const feedback = card.getByText('收藏回写已开启');
  await expect(feedback).toBeVisible();
  const controlCenters = await card.evaluate((root) => {
    const elements = [
      root.querySelector('.switch-field input'),
      root.querySelector('.switch-field > span'),
      root.querySelector('.settings-feedback')
    ];
    if (elements.some((element) => !element)) {
      throw new Error('收藏回写控件未完整渲染');
    }
    return elements.map((element) => {
      const box = element!.getBoundingClientRect();
      return box.top + box.height / 2;
    });
  });
  expect(
    Math.max(...controlCenters) - Math.min(...controlCenters)
  ).toBeLessThanOrEqual(1);
  await toggle.uncheck();
  await expect(page.getByText('收藏回写已关闭')).toBeVisible();
  await expect(
    page.getByRole('button', { name: '保存收藏回写设置' })
  ).toHaveCount(0);
});

test('bookmark write-back saving does not disable credential controls', async ({
  page
}) => {
  await mockSystem(page);
  await page.unroute('**/api/pixiv/account/bookmark-writeback');
  let reportStarted!: () => void;
  const started = new Promise<void>((resolve) => {
    reportStarted = resolve;
  });
  let releaseRequest!: () => void;
  const requestRelease = new Promise<void>((resolve) => {
    releaseRequest = resolve;
  });
  await page.route('**/api/pixiv/account/bookmark-writeback', async (route) => {
    reportStarted();
    await requestRelease;
    await fulfillJson(route, 200, {
      account_id: accountId,
      pixiv_user_id: 10001,
      display_name: 'Test Artist',
      avatar_url: null,
      state: 'restricted',
      bookmark_writeback_enabled: true,
      last_validated_at: '2026-07-30T02:00:00Z',
      revision: 5
    });
  });

  await page.goto('/system/account');
  const toggle = page.getByLabel('同步修改Pixiv收藏');
  await toggle.check();
  await started;

  await expect(toggle).toBeEnabled();
  await expect(page.getByRole('button', { name: '保存并验证' })).toBeEnabled();
  await expect(page.getByRole('button', { name: '重新验证' })).toBeEnabled();
  await expect(
    page.getByRole('button', { name: '清除Pixiv Cookie' })
  ).toBeEnabled();

  releaseRequest();
  await expect(page.getByText('收藏回写已开启')).toBeVisible();
});

test('bookmark write-back applies rapid changes in order and reports the final state', async ({
  page
}) => {
  await mockSystem(page);
  await page.unroute('**/api/pixiv/account/bookmark-writeback');
  const requests: Array<Record<string, unknown>> = [];
  let revision = 4;
  await page.route('**/api/pixiv/account/bookmark-writeback', async (route) => {
    const request = route.request().postDataJSON() as Record<string, unknown>;
    requests.push(request);
    if (requests.length === 1) {
      await new Promise((resolve) => setTimeout(resolve, 100));
    }
    revision += 1;
    await fulfillJson(route, 200, {
      account_id: accountId,
      pixiv_user_id: 10001,
      display_name: 'Test Artist',
      avatar_url: null,
      state: 'restricted',
      bookmark_writeback_enabled: Boolean(request.enabled),
      last_validated_at: '2026-07-30T02:00:00Z',
      revision
    });
  });

  await page.goto('/system/account');
  const toggle = page.getByLabel('同步修改Pixiv收藏');
  await toggle.check();
  await toggle.uncheck();

  await expect(page.getByText('收藏回写已关闭')).toBeVisible();
  await expect(page.getByText('收藏回写已开启')).toHaveCount(0);
  expect(requests).toEqual([
    {
      expected_account_id: accountId,
      enabled: true,
      expected_revision: 4
    },
    {
      expected_account_id: accountId,
      enabled: false,
      expected_revision: 5
    }
  ]);
});

test('bookmark write-back feedback survives the account event caused by its own update', async ({
  page
}) => {
  const api = await mockSystem(page);
  await page.unroute('**/api/events');
  let publishAccountEvent!: () => void;
  const accountEvent = new Promise<void>((resolve) => {
    publishAccountEvent = resolve;
  });
  await page.route('**/api/events', async (route) => {
    await accountEvent;
    await route.fulfill({
      status: 200,
      headers: { 'Content-Type': 'text/event-stream' },
      body: `event: app_event\ndata: {"resource":"pixiv_account","resource_id":"${accountId}"}\n\n`
    });
  });

  await page.goto('/system/account');
  const accountLoadsBeforeUpdate = api.accountLoads;
  await page.getByLabel('同步修改Pixiv收藏').check();
  await expect(page.getByText('收藏回写已开启')).toBeVisible();
  publishAccountEvent();
  await expect
    .poll(() => api.accountLoads)
    .toBeGreaterThan(accountLoadsBeforeUpdate);
  await expect(page.getByText('收藏回写已开启')).toBeVisible();
});

test('favorites sync reports both enabled and disabled changes in its settings card', async ({
  page
}) => {
  const api = await mockSystem(page);
  await page.setViewportSize({ width: 1280, height: 720 });
  await page.goto('/system/account');

  const card = page.locator('section[data-settings-card]').filter({
    hasText: '收藏同步'
  });
  const toggle = card.getByLabel('启用收藏同步');
  const desktopLayout = await card
    .locator('.subscription-sync-controls')
    .evaluate((row) => {
      const children = [...row.children];
      const bottoms = children.map((child) => {
        const box = child.getBoundingClientRect();
        return box.bottom;
      });
      return {
        columns: getComputedStyle(row).gridTemplateColumns.split(' ').length,
        bottomDelta: Math.max(...bottoms) - Math.min(...bottoms),
        overflow: row.scrollWidth - row.clientWidth
      };
    });
  expect(desktopLayout.columns).toBe(3);
  expect(desktopLayout.bottomDelta).toBeLessThanOrEqual(1);
  expect(desktopLayout.overflow).toBeLessThanOrEqual(1);
  await expect(
    card.locator('.sync-command').getByRole('button', {
      name: '完整同步'
    })
  ).toBeVisible();
  await expect(card.locator('.sync-command')).toContainText('上次完整核对：');

  await expect(toggle).not.toBeChecked();
  await toggle.check();
  await expect(page.getByText('收藏同步已开启')).toBeVisible();
  await expect.poll(() => api.favoriteSetting).toMatchObject({ enabled: true });
  const controlCenters = await card.evaluate((root) => {
    const elements = [
      root.querySelector('.switch-field input'),
      root.querySelector('.switch-field strong'),
      root.querySelector('.settings-feedback'),
      root.querySelector('.sync-command button')
    ];
    if (elements.some((element) => !element)) {
      throw new Error('收藏同步控件未完整渲染');
    }
    return elements.map((element) => {
      const box = element!.getBoundingClientRect();
      return box.top + box.height / 2;
    });
  });
  expect(
    Math.max(...controlCenters) - Math.min(...controlCenters)
  ).toBeLessThanOrEqual(1);
  await toggle.uncheck();
  await expect(page.getByText('收藏同步已关闭')).toBeVisible();
  await expect
    .poll(() => api.favoriteSetting)
    .toMatchObject({ enabled: false });

  await page.setViewportSize({ width: 760, height: 900 });
  const compactLayout = await card
    .locator('.subscription-sync-controls')
    .evaluate((row) => ({
      columns: getComputedStyle(row).gridTemplateColumns.split(' ').length,
      overflow: row.scrollWidth - row.clientWidth
    }));
  expect(compactLayout).toEqual({ columns: 1, overflow: 0 });
});

test('favorites sync keeps its latest intent without changing credential controls', async ({
  page
}) => {
  await mockSystem(page);
  await page.unroute('**/api/favorites');
  let reportStarted!: () => void;
  const started = new Promise<void>((resolve) => {
    reportStarted = resolve;
  });
  let releaseRequest!: () => void;
  const requestRelease = new Promise<void>((resolve) => {
    releaseRequest = resolve;
  });
  await page.route('**/api/favorites', async (route) => {
    if (route.request().method() === 'GET') {
      await fulfillJson(route, 200, favoriteState(false, 9));
      return;
    }
    reportStarted();
    await requestRelease;
    await fulfillJson(route, 200, favoriteState(true, 10));
  });

  await page.goto('/system/account');
  const toggle = page.getByLabel('启用收藏同步');
  await toggle.check();
  await started;

  await expect(toggle).toBeChecked();
  await expect(toggle).toBeEnabled();
  await expect(page.getByRole('button', { name: '保存并验证' })).toBeEnabled();
  await expect(page.getByRole('button', { name: '重新验证' })).toBeEnabled();
  await expect(
    page.getByRole('button', { name: '清除Pixiv Cookie' })
  ).toBeEnabled();

  releaseRequest();
  await expect(page.getByText('收藏同步已开启')).toBeVisible();
});

test('credentials panel clears only the saved Pixiv Cookie through the shared confirmation', async ({
  page
}) => {
  const api = await mockSystem(page);
  await page.goto('/system/account');

  const credentials = page
    .getByRole('heading', { name: '凭据' })
    .locator('xpath=ancestor::section[@data-settings-card]');
  await credentials.getByRole('button', { name: '清除Pixiv Cookie' }).click();
  const dialog = page.getByRole('dialog', { name: '清除Pixiv Cookie？' });
  await expect(dialog).toContainText(
    '加密保存的Cookie将被删除，账户资料、已归档作品、订阅和历史记录会保留。'
  );
  await dialog.getByRole('button', { name: '清除Cookie' }).click();

  await expect
    .poll(() => api.credentialClear)
    .toEqual({
      expected_account_id: accountId,
      expected_revision: 4
    });
  await expect(page.getByText('Pixiv Cookie已清除')).toBeVisible();
  await expect(dialog).toHaveCount(0);
  await expect(page.getByText('未配置', { exact: true })).toBeVisible();
  await expect(page.getByText('Test Artist')).toHaveCount(0);
  await expect(page.getByText('10001', { exact: true })).toHaveCount(0);
  await expect(page.getByLabel('同步修改Pixiv收藏')).toBeDisabled();
  await expect(page.getByLabel('启用收藏同步')).toBeDisabled();
  await expect(page.getByRole('button', { name: '完整同步' })).toBeDisabled();
});

test('top account menu reuses cookie clearing behavior and a readable surface', async ({
  page
}) => {
  const api = await mockSystem(page);
  await page.goto('/overview');
  await page.getByRole('button', { name: '管理员菜单' }).click();

  const popover = page.locator('[data-topbar-popover="account"]');
  const backgroundAlpha = await popover.evaluate((element) => {
    const color = getComputedStyle(element).backgroundColor;
    const match = color.match(/rgba?\([^,]+,[^,]+,[^,]+(?:,\s*([\d.]+))?\)/);
    return match?.[1] ? Number(match[1]) : 1;
  });
  expect(backgroundAlpha).toBeGreaterThanOrEqual(0.93);

  await popover.getByRole('button', { name: '清除Pixiv Cookie' }).click();
  const dialog = page.getByRole('dialog', { name: '清除Pixiv Cookie？' });
  await expect(dialog).toContainText(
    '加密保存的Cookie将被删除，账户资料、已归档作品、订阅和历史记录会保留。'
  );
  await dialog.getByRole('button', { name: '清除Cookie' }).click();

  await expect
    .poll(() => api.credentialClear)
    .toEqual({
      expected_account_id: accountId,
      expected_revision: 4
    });
  await expect(dialog).toHaveCount(0);
  await expect(popover.getByText('Pixiv Cookie已清除')).toBeVisible();
  await expect(popover.getByText('管理员', { exact: true })).toBeVisible();
  await expect(popover.getByText('本地账户', { exact: true })).toBeVisible();
  await expect(popover.getByText('Test Artist')).toHaveCount(0);
});

test('cookie clearing closes confirmation while an account event reload is pending', async ({
  page
}) => {
  await mockSystem(page);
  await page.unroute('**/api/events');
  await page.unroute('**/api/pixiv/account');
  await page.unroute('**/api/pixiv/account/credential');

  let publishAccountEvent!: () => void;
  const accountEvent = new Promise<void>((resolve) => {
    publishAccountEvent = resolve;
  });
  let reportReloadStarted!: () => void;
  const reloadStarted = new Promise<void>((resolve) => {
    reportReloadStarted = resolve;
  });
  let releaseReload!: () => void;
  const reloadRelease = new Promise<void>((resolve) => {
    releaseReload = resolve;
  });
  let accountRequests = 0;
  let eventConnections = 0;
  let clearRequest: Record<string, unknown> | undefined;

  await page.route('**/api/events', async (route) => {
    eventConnections += 1;
    if (eventConnections > 1) {
      await route.fulfill({ status: 204 });
      return;
    }
    await accountEvent;
    await route.fulfill({
      status: 200,
      headers: { 'Content-Type': 'text/event-stream' },
      body: `event: app_event\ndata: {"resource":"pixiv_account","resource_id":"${accountId}"}\n\n`
    });
  });
  await page.route('**/api/pixiv/account', async (route) => {
    accountRequests += 1;
    if (accountRequests > 1) {
      reportReloadStarted();
      await reloadRelease;
    }
    await fulfillJson(route, 200, {
      account_id: accountId,
      pixiv_user_id: 10001,
      display_name: 'Test Artist',
      avatar_url: null,
      state: accountRequests > 1 ? 'unconfigured' : 'restricted',
      bookmark_writeback_enabled: false,
      last_validated_at: accountRequests > 1 ? null : '2026-07-30T02:00:00Z',
      revision: accountRequests > 1 ? 5 : 4
    });
  });
  await page.route('**/api/pixiv/account/credential', async (route) => {
    clearRequest = route.request().postDataJSON();
    publishAccountEvent();
    await reloadStarted;
    await fulfillJson(route, 200, {
      account_id: accountId,
      pixiv_user_id: 10001,
      display_name: 'Test Artist',
      avatar_url: null,
      state: 'unconfigured',
      bookmark_writeback_enabled: false,
      last_validated_at: null,
      revision: 5
    });
  });

  await page.goto('/system/account');
  await page.getByRole('button', { name: '清除Pixiv Cookie' }).click();
  const dialog = page.getByRole('dialog', { name: '清除Pixiv Cookie？' });
  await dialog.getByRole('button', { name: '清除Cookie' }).click();
  await expect
    .poll(() => clearRequest)
    .toEqual({
      expected_account_id: accountId,
      expected_revision: 4
    });
  releaseReload();

  await expect(dialog).toHaveCount(0);
  await expect(page.getByText('Pixiv Cookie已清除')).toBeVisible();
});

test('credential feedback clears when a later account revision is loaded', async ({
  page
}) => {
  await mockSystem(page);
  await page.unroute('**/api/events');
  await page.unroute('**/api/pixiv/account');
  await page.unroute('**/api/pixiv/account/credential');

  let publishAccountEvent!: () => void;
  const accountEvent = new Promise<void>((resolve) => {
    publishAccountEvent = resolve;
  });
  let accountState = 'restricted';
  let accountRevision = 4;

  await page.route('**/api/events', async (route) => {
    await accountEvent;
    await route.fulfill({
      status: 200,
      headers: { 'Content-Type': 'text/event-stream' },
      body: `event: app_event\ndata: {"resource":"pixiv_account","resource_id":"${accountId}"}\n\n`
    });
  });
  await page.route('**/api/pixiv/account', (route) =>
    fulfillJson(route, 200, {
      account_id: accountId,
      pixiv_user_id: 10001,
      display_name: 'Test Artist',
      avatar_url: null,
      state: accountState,
      bookmark_writeback_enabled: false,
      last_validated_at:
        accountState === 'unconfigured' ? null : '2026-07-30T02:00:00Z',
      revision: accountRevision
    })
  );
  await page.route('**/api/pixiv/account/credential', async (route) => {
    accountState = 'unconfigured';
    accountRevision = 5;
    await fulfillJson(route, 200, {
      account_id: accountId,
      pixiv_user_id: 10001,
      display_name: 'Test Artist',
      avatar_url: null,
      state: accountState,
      bookmark_writeback_enabled: false,
      last_validated_at: null,
      revision: accountRevision
    });
  });

  await page.goto('/system/account');
  await page.getByRole('button', { name: '清除Pixiv Cookie' }).click();
  await page
    .getByRole('dialog', { name: '清除Pixiv Cookie？' })
    .getByRole('button', { name: '清除Cookie' })
    .click();
  await expect(page.getByText('Pixiv Cookie已清除')).toBeVisible();

  accountState = 'restricted';
  accountRevision = 6;
  publishAccountEvent();

  await expect(page.getByText('受限访问', { exact: true })).toBeVisible();
  await expect(page.getByText('Pixiv Cookie已清除')).toHaveCount(0);
});

test('favorites stays empty and explains an unconfigured account', async ({
  page
}) => {
  await mockApi(page);
  await page.unroute('**/api/pixiv/account');
  let galleryRequests = 0;
  await page.route('**/api/pixiv/account', async (route) => {
    await fulfillJson(route, 200, {
      account_id: accountId,
      pixiv_user_id: 10001,
      display_name: 'Historical Account',
      avatar_url: null,
      state: 'unconfigured',
      bookmark_writeback_enabled: true,
      last_validated_at: null,
      revision: 5
    });
  });
  await page.route('**/api/gallery/search', async (route) => {
    galleryRequests += 1;
    await fulfillJson(route, 200, { items: [], next_cursor: null, total: 0 });
  });

  await page.goto('/gallery/favorites');

  await expect(
    page.getByText('尚未配置Pixiv账户', { exact: true })
  ).toBeVisible();
  await expect(
    page.getByText('配置并验证Pixiv Cookie后，才能查看这个账户的收藏。')
  ).toBeVisible();
  expect(galleryRequests).toBe(0);
  await expect(page.getByText('Historical Account')).toHaveCount(0);
});

test('favorites keeps its cached state while a failed refresh is retried', async ({
  page
}) => {
  const api = await mockSystem(page);
  await page.goto('/system/account');
  await expect(page.getByText('上次完整核对：')).toBeVisible();
  await expect(
    page.getByRole('button', { name: '重新读取收藏同步状态' })
  ).toHaveCount(0);

  api.blockFavoriteLoads();
  await page.evaluate(() =>
    document.dispatchEvent(new Event('visibilitychange'))
  );
  await expect(page.getByText('收藏同步状态暂时无法读取')).toBeVisible();
  await expect(page.getByText('上次完整核对：')).toBeVisible();

  api.allowFavoriteLoads();
  await page.getByRole('button', { name: '重新读取收藏同步状态' }).click();
  await expect(page.getByText('收藏同步状态暂时无法读取')).toHaveCount(0);
  await expect(
    page.getByRole('button', { name: '重新读取收藏同步状态' })
  ).toHaveCount(0);
  await expect(page.getByLabel('启用收藏同步')).toBeEnabled();
});

test('system secondary navigation opens trash and settings without route errors', async ({
  page
}) => {
  await mockSystem(page);
  const pageErrors: string[] = [];
  page.on('pageerror', (error) => pageErrors.push(error.message));
  await page.route(/\/api\/trash(?:\?.*)?$/, async (route) => {
    await fulfillJson(route, 200, {
      items: [],
      next_cursor: null,
      summary: {
        total_count: 0,
        logical_bytes: 0,
        estimated_reclaimable_bytes: 0
      },
      all_summary: {
        total_count: 0,
        logical_bytes: 0,
        estimated_reclaimable_bytes: 0
      }
    });
  });
  await page.goto('/system/account');
  const navigation = page.getByRole('navigation', { name: '二级导航' });
  await navigation.getByRole('link', { name: '回收站' }).click();
  await expect(page).toHaveURL('/system/trash');
  await expect(page.getByRole('heading', { name: '回收站' })).toBeVisible();
  await expect(page.getByText('回收站是空的')).toBeVisible();

  await navigation.getByRole('link', { name: '系统设置' }).click();
  await expect(page).toHaveURL('/system/settings');
  await expect(page.getByRole('heading', { name: '系统设置' })).toBeVisible();
  await expect(page.getByText('系统设置暂时无法读取')).toHaveCount(0);
  expect(pageErrors).toEqual([]);
});

test('storage and maintenance expose protection thresholds and capabilities', async ({
  page
}) => {
  const api = await mockSystem(page);
  await page.goto('/system/settings');

  await expect(page.getByRole('heading', { name: '存储状态' })).toBeVisible();
  await expect(page.getByText('媒体写入已停止')).toBeVisible();
  await expect(page.getByText(/WebP\s*可用/)).toBeVisible();
  await expect(page.getByText(/AVIF\s*不可用/)).toBeVisible();
  await expect(
    page.getByText('媒体目录大小', { exact: true }).locator('..')
  ).toContainText('1.0 MiB');
  expect(
    await page.locator('.capacity-values > div > span').allTextContents()
  ).toEqual(['可用空间', '总容量', '媒体目录大小', '预警阈值', '停止写入阈值']);
  await expect(page.getByLabel('图片存储目录')).toHaveValue(
    '/srv/pixivarchive/media'
  );
  await page.getByLabel('图片存储目录').fill('/mnt/archive/pixiv');
  await page.getByRole('button', { name: '保存存储设置' }).click();
  await expect
    .poll(() => api.settingUpdates.storage)
    .toMatchObject({
      expected_revision: 2,
      value: { media_root: '/mnt/archive/pixiv' }
    });
  await expect(
    page.getByText('图片目录将在Web和Worker重启后生效')
  ).toBeVisible();

  await expect(page.getByRole('heading', { name: '后台维护' })).toBeVisible();
  const maintenanceLayout = await page
    .locator('.maintenance-list')
    .evaluate((list) => {
      const rows = [...list.children];
      const buttons = rows.map((row) =>
        row.querySelector('button')!.getBoundingClientRect()
      );
      const labels = rows.map((row) =>
        row.querySelector('strong')!.getBoundingClientRect()
      );
      return {
        buttonWidths: buttons.map((button) => button.width),
        centerDeltas: buttons.map((button, index) =>
          Math.abs(
            button.y +
              button.height / 2 -
              (labels[index].y + labels[index].height / 2)
          )
        )
      };
    });
  expect(
    Math.max(...maintenanceLayout.buttonWidths) -
      Math.min(...maintenanceLayout.buttonWidths)
  ).toBeLessThanOrEqual(1);
  expect(Math.max(...maintenanceLayout.centerDeltas)).toBeLessThanOrEqual(1);
  await expect(page.getByText('regenerate_derivatives')).toHaveCount(0);
  await expect(page.getByText('scan_expired_trash')).toHaveCount(0);
  await page.getByRole('button', { name: '重新生成浏览图' }).click();
  await expect
    .poll(() => api.maintenanceRequests)
    .toEqual([{ operation: 'regenerate_derivatives' }]);
  await expect(page.getByText('维护任务已加入后台队列')).toBeVisible();

  await page.getByRole('button', { name: '扫描到期作品' }).click();
  await expect(page.getByText('当前没有符合条件的项目')).toBeVisible();
});

test('media usage failure keeps filesystem capacity values visible', async ({
  page
}) => {
  await mockSystem(page, { storageUsageUnavailable: true });
  await page.goto('/system/settings');

  await expect(
    page.getByText('可用空间', { exact: true }).locator('..')
  ).not.toContainText('—');
  await expect(
    page.getByText('总容量', { exact: true }).locator('..')
  ).not.toContainText('—');
  await expect(
    page.getByText('媒体目录大小', { exact: true }).locator('..')
  ).toContainText('—');
});

test('filesystem capacity appears while media usage is still loading', async ({
  page
}) => {
  await mockSystem(page);
  let releaseUsage: (() => void) | undefined;
  const usageReady = new Promise<void>((resolve) => {
    releaseUsage = resolve;
  });
  await page.unroute('**/api/system/storage-usage');
  await page.route('**/api/system/storage-usage', async (route) => {
    await usageReady;
    await fulfillJson(route, 200, { media_directory_bytes: 1_048_576 });
  });

  await page.goto('/system/settings');
  await expect(
    page.getByText('可用空间', { exact: true }).locator('..')
  ).not.toContainText('—');
  await expect(
    page.getByText('媒体目录大小', { exact: true }).locator('..')
  ).toContainText('—');

  releaseUsage!();
  await expect(
    page.getByText('媒体目录大小', { exact: true }).locator('..')
  ).toContainText('1.0 MiB');
});

test('system settings save queue quotas without exposing task priorities', async ({
  page
}) => {
  const api = await mockSystem(page);
  await page.goto('/system/settings');

  await expect(page.getByRole('heading', { name: '系统设置' })).toBeVisible();
  await expect(
    page.getByRole('heading', { name: '任务队列配额' })
  ).toBeVisible();
  await expect(page.getByRole('heading', { name: '四级队列配额' })).toHaveCount(
    0
  );
  await expect(
    page.getByRole('heading', { name: '任务类型优先级' })
  ).toHaveCount(0);
  await expect(page.getByLabel('作品导入优先级')).toHaveCount(0);

  const queueCard = page
    .getByRole('heading', { name: '任务队列配额' })
    .locator('xpath=ancestor::section[@data-settings-card]');
  const processingCard = page
    .getByRole('heading', { name: '请求与处理限制' })
    .locator('xpath=ancestor::section[@data-settings-card]');
  await expect(queueCard).toHaveCount(1);
  await expect(processingCard).toHaveCount(1);
  const cardSurface = (element: HTMLElement) => {
    const style = getComputedStyle(element);
    return {
      backgroundColor: style.backgroundColor,
      borderColor: style.borderColor,
      borderRadius: style.borderRadius,
      boxShadow: style.boxShadow
    };
  };
  expect(await queueCard.evaluate(cardSurface)).toEqual(
    await processingCard.evaluate(cardSurface)
  );

  const queueButton = page.getByRole('button', { name: '保存队列设置' });
  const processingButton = page.getByRole('button', {
    name: '保存处理限制'
  });
  await expect(queueButton).toHaveClass(/secondary-button/);
  await expect(processingButton).toHaveClass(/secondary-button/);

  await page.getByLabel('即时操作配额').fill('10');
  await page.getByLabel('后台维护配额').fill('2');
  await page.getByRole('button', { name: '保存队列设置' }).click();
  await expect
    .poll(() => api.settingUpdates.queue)
    .toMatchObject({
      expected_revision: 4,
      value: {
        quota_weights: {
          immediate: 10,
          manual_import: 8,
          scheduled_collection: 2,
          background_maintenance: 2
        }
      }
    });
  expect(
    (
      api.settingUpdates.queue.value as {
        job_priorities: Array<{ job_kind: string; priority: string }>;
      }
    ).job_priorities
  ).toEqual(jobPriorities());
  await expect(page.getByText(/TOTP/)).toHaveCount(0);
  await expect(page.getByText(/管理员密码/)).toHaveCount(0);
});

test('content settings keep masking exclusive and randomize only saved choices', async ({
  page
}) => {
  const api = await mockSystem(page);
  await page.unroute('**/api/gallery/overview-decorations**');
  const decorationRequests: Array<{ method: string; date: string | null }> = [];
  await page.route('**/api/gallery/overview-decorations**', async (route) => {
    const request = route.request();
    decorationRequests.push({
      method: request.method(),
      date: new URL(request.url()).searchParams.get('date')
    });
    await fulfillJson(route, 200, { items: [null, null, null] });
  });
  await page.goto('/system/settings');

  const allowNsfw = page.getByLabel('概览装饰图允许R-18内容');
  const maskThumbnails = page.getByLabel('遮挡非全年龄缩略图');
  const save = page.getByRole('button', { name: '保存显示设置' });
  const shuffle = page.getByRole('button', {
    name: '重新随机概览装饰图'
  });

  await expect(page.getByText('R-18、R-18G及分级未知作品')).toBeVisible();
  await expect(save).toBeDisabled();
  await expect(shuffle).toBeEnabled();

  await allowNsfw.check();
  await expect(save).toBeEnabled();
  await expect(shuffle).toBeDisabled();
  await maskThumbnails.check();
  await expect(allowNsfw).not.toBeChecked();
  await expect(allowNsfw).toBeDisabled();
  await maskThumbnails.uncheck();
  await expect(allowNsfw).not.toBeChecked();
  await expect(allowNsfw).toBeEnabled();
  await expect(save).toBeDisabled();
  await allowNsfw.check();

  await save.click();
  await expect
    .poll(() => api.settingUpdates.content)
    .toEqual({
      expected_revision: 2,
      value: {
        overview_allow_nsfw: true,
        mask_non_all_age_thumbnails: false
      }
    });
  await expect(shuffle).toBeEnabled();
  await shuffle.click();
  const localDate = await page.evaluate(() => {
    const now = new Date();
    const year = now.getFullYear();
    const month = String(now.getMonth() + 1).padStart(2, '0');
    const day = String(now.getDate()).padStart(2, '0');
    return `${year}-${month}-${day}`;
  });
  await expect
    .poll(() => decorationRequests)
    .toEqual([{ method: 'POST', date: localDate }]);
  await expect(page.getByText('概览装饰图已经重新选择')).toBeVisible();
});

test('about page exposes the application version and source repository', async ({
  page
}) => {
  await mockSystem(page);
  await page.goto('/system/about');

  const versionRow = page.getByText('版本', { exact: true }).locator('..');
  await expect(versionRow.getByText('v0.1.0')).toBeVisible();
  const repositoryLink = versionRow.getByRole('link', { name: 'GitHub仓库' });
  await expect(repositoryLink).toHaveAttribute(
    'href',
    'https://github.com/Mizuno-Sachiko/PixivArchive'
  );
  await expect(repositoryLink.locator('svg')).toBeVisible();
  await expect(page.getByText('源码仓库', { exact: true })).toHaveCount(0);
});

test('about page keeps a single line placeholder until the version is loaded', async ({
  page
}) => {
  await mockSystem(page);
  let releaseStatus: (() => void) | undefined;
  const statusReady = new Promise<void>((resolve) => {
    releaseStatus = resolve;
  });
  await page.unroute('**/api/system/status');
  await page.route('**/api/system/status', async (route) => {
    await statusReady;
    await fulfillJson(route, 200, {
      version: '0.1.0',
      git_commit: 'test',
      migration_version: 1,
      database: { status: 'healthy', message: null },
      media: { status: 'healthy', message: null },
      worker: { status: 'healthy', message: null },
      queue: {},
      setting_revisions: {},
      storage: {
        active_media_root: '/srv/media',
        total_bytes: 100,
        available_bytes: 50,
        warning_threshold_bytes: 10,
        write_stop_threshold_bytes: 5,
        write_stopped: false
      },
      capabilities: {
        webp_derivatives: true,
        avif_derivatives: false,
        reflink: false
      }
    });
  });

  await page.goto('/system/about');
  const versionRow = page.getByText('版本', { exact: true }).locator('..');
  const placeholder = versionRow.getByLabel('正在读取版本');
  await expect(placeholder).toHaveText('');
  await expect(placeholder).toHaveCSS('height', '1px');
  expect(
    await placeholder.evaluate(
      (element) => element.getBoundingClientRect().width
    )
  ).toBeGreaterThan(40);
  await expect(versionRow.getByText('v0.1.0')).toHaveCount(0);

  releaseStatus!();
  await expect(versionRow.getByText('v0.1.0')).toBeVisible();
  await expect(versionRow.getByLabel('正在读取版本')).toHaveCount(0);
});

test('about page spaces version metadata evenly', async ({ page }) => {
  await mockSystem(page);
  await page.goto('/system/about');

  const versionRow = page.getByText('版本', { exact: true }).locator('..');
  const versionLabel = versionRow.getByText('版本', { exact: true });
  const versionValue = versionRow.getByText('v0.1.0');
  const repositoryLink = versionRow.getByRole('link', { name: 'GitHub仓库' });
  const [labelBox, valueBox, repositoryBox] = await Promise.all([
    versionLabel.boundingBox(),
    versionValue.boundingBox(),
    repositoryLink.boundingBox()
  ]);

  expect(labelBox).not.toBeNull();
  expect(valueBox).not.toBeNull();
  expect(repositoryBox).not.toBeNull();

  const labelGap = valueBox!.x - labelBox!.x - labelBox!.width;
  const repositoryGap = repositoryBox!.x - valueBox!.x - valueBox!.width;
  const expectedGap = await page.evaluate(
    () => parseFloat(getComputedStyle(document.documentElement).fontSize) * 8
  );
  expect(Math.abs(labelGap - expectedGap)).toBeLessThanOrEqual(1);
  expect(Math.abs(repositoryGap - expectedGap)).toBeLessThanOrEqual(1);
  expect(Math.abs(labelGap - repositoryGap)).toBeLessThanOrEqual(1);
});

test('about page keeps the source repository available when status loading fails', async ({
  page
}) => {
  await mockSystem(page, { statusUnavailable: true });
  await page.goto('/system/about');

  const versionRow = page.getByText('版本', { exact: true }).locator('..');
  await expect(versionRow.getByText('版本信息暂时无法读取')).toBeVisible();
  await expect(
    versionRow.getByRole('link', { name: 'GitHub仓库' })
  ).toHaveAttribute('href', 'https://github.com/Mizuno-Sachiko/PixivArchive');
});

interface SystemMockState {
  accountUpdate?: Record<string, unknown>;
  accountLoads: number;
  bookmarkSetting?: Record<string, unknown>;
  favoriteSetting?: Record<string, unknown>;
  credentialClear?: Record<string, unknown>;
  maintenanceRequests: Array<Record<string, unknown>>;
  settingUpdates: Record<string, Record<string, unknown>>;
  blockFavoriteLoads: () => void;
  allowFavoriteLoads: () => void;
}

interface SystemMockOptions {
  statusUnavailable?: boolean;
  storageUsageUnavailable?: boolean;
}

function favoriteState(enabled: boolean, revision: number) {
  return {
    last_full_reconciled_at: '2026-07-30T03:00:00Z',
    subscription: {
      id: '0198f653-0000-7000-8000-000000000011',
      account_id: accountId,
      account_pixiv_user_id: 10001,
      account_avatar_url: null,
      account_state: 'restricted',
      rule_id: null,
      name: '收藏同步',
      kind: 'bookmarks',
      enabled,
      schedule: { interval_minutes: 30, lookback_pages: 2 },
      params: { mode: 'all', visibility: 'all' },
      next_run_at: '2026-07-30T03:30:00Z',
      pending_run: false,
      recent_state: 'succeeded',
      revision
    }
  };
}

async function mockSystem(
  page: Page,
  options: SystemMockOptions = {}
): Promise<SystemMockState> {
  await mockApi(page);
  let favoriteLoadsBlocked = false;
  const state: SystemMockState = {
    accountLoads: 0,
    maintenanceRequests: [],
    settingUpdates: {},
    blockFavoriteLoads: () => (favoriteLoadsBlocked = true),
    allowFavoriteLoads: () => (favoriteLoadsBlocked = false)
  };
  let accountState = 'restricted';
  let bookmarkWriteback = false;
  let favoriteEnabled = false;
  let accountRevision = 4;

  await page.route('**/api/pixiv/account', async (route) => {
    if (route.request().method() === 'PUT') {
      state.accountUpdate = route.request().postDataJSON();
      accountState = 'credential_invalid';
    } else {
      state.accountLoads += 1;
    }
    await fulfillJson(route, 200, {
      account_id: accountId,
      pixiv_user_id: 10001,
      display_name: 'Test Artist',
      state: accountState,
      bookmark_writeback_enabled: bookmarkWriteback,
      last_validated_at: '2026-07-30T02:00:00Z',
      revision: accountRevision
    });
  });

  await page.route('**/api/pixiv/account/credential', async (route) => {
    state.credentialClear = route.request().postDataJSON();
    accountState = 'unconfigured';
    accountRevision += 1;
    await fulfillJson(route, 200, {
      account_id: accountId,
      pixiv_user_id: 10001,
      display_name: 'Test Artist',
      avatar_url: null,
      state: accountState,
      bookmark_writeback_enabled: bookmarkWriteback,
      last_validated_at: null,
      revision: accountRevision
    });
  });

  await page.route('**/api/pixiv/account/bookmark-writeback', async (route) => {
    const request = route.request().postDataJSON() as Record<string, unknown>;
    state.bookmarkSetting = request;
    bookmarkWriteback = Boolean(request.enabled);
    accountRevision += 1;
    await fulfillJson(route, 200, {
      account_id: accountId,
      pixiv_user_id: 10001,
      display_name: 'Test Artist',
      state: accountState,
      bookmark_writeback_enabled: bookmarkWriteback,
      last_validated_at: '2026-07-30T02:00:00Z',
      revision: accountRevision
    });
  });

  await page.route(/\/api\/favorites$/, async (route) => {
    if (favoriteLoadsBlocked) {
      await fulfillJson(route, 503, { error: 'unavailable' });
      return;
    }
    if (route.request().method() === 'PUT') {
      const request = route.request().postDataJSON() as Record<string, unknown>;
      state.favoriteSetting = request;
      favoriteEnabled = Boolean(request.enabled);
    }
    await fulfillJson(route, 200, {
      last_full_reconciled_at: '2026-07-30T03:00:00Z',
      subscription: {
        id: '0198f653-0000-7000-8000-000000000011',
        account_id: accountId,
        account_pixiv_user_id: 10001,
        account_avatar_url: null,
        account_state: accountState,
        rule_id: null,
        name: '收藏同步',
        kind: 'bookmarks',
        enabled: favoriteEnabled,
        schedule: { interval_minutes: 30, lookback_pages: 2 },
        params: { mode: 'all', visibility: 'all' },
        next_run_at: '2026-07-30T03:30:00Z',
        pending_run: false,
        recent_state: 'succeeded',
        revision: 2
      }
    });
  });

  await page.route('**/api/system/status', async (route) => {
    if (options.statusUnavailable) {
      await fulfillJson(route, 503, { error: 'unavailable' });
      return;
    }
    await fulfillJson(route, 200, {
      version: '0.1.0',
      git_commit: '8989416',
      migration_version: 14,
      database: { status: 'healthy', message: null },
      worker: { status: 'healthy', message: null },
      media: {
        status: 'write_stopped',
        message: '剩余空间低于32 GiB'
      },
      storage: {
        active_media_root: '/srv/pixivarchive/media',
        total_bytes: 5_000_000_000_000,
        available_bytes: 29_000_000_000,
        warning_threshold_bytes: 107_374_182_400,
        write_stop_threshold_bytes: 34_359_738_368,
        write_stopped: true
      },
      capabilities: {
        webp_derivatives: true,
        avif_derivatives: false,
        reflink: true
      },
      queue: {
        immediate: { queued: 1, running: 0 },
        manual_import: { queued: 2, running: 0 },
        scheduled_collection: { queued: 3, running: 1 },
        background_maintenance: { queued: 4, running: 1 }
      },
      setting_revisions: {
        queue: 4,
        storage: 2,
        retry: 1,
        security: 1,
        content: 2
      }
    });
  });

  await page.route('**/api/system/settings', async (route) => {
    await fulfillJson(route, 200, { value: settings() });
  });

  await page.route('**/api/system/storage-usage', async (route) => {
    if (options.storageUsageUnavailable) {
      await fulfillJson(route, 503, { error: 'unavailable' });
      return;
    }
    await fulfillJson(route, 200, { media_directory_bytes: 1_048_576 });
  });

  await page.route('**/api/system/settings/*', async (route) => {
    const group = route.request().url().split('/').at(-1) ?? '';
    state.settingUpdates[group] = route.request().postDataJSON();
    await fulfillJson(route, 200, { group, revision: 5 });
  });

  await page.route('**/api/system/maintenance', async (route) => {
    const request = route.request().postDataJSON();
    state.maintenanceRequests.push(request);
    await fulfillJson(route, 202, {
      operation: request.operation,
      job_ids:
        request.operation === 'scan_expired_trash'
          ? []
          : ['0198f653-0000-7000-8000-000000000030'],
      queued_count: request.operation === 'scan_expired_trash' ? 0 : 1
    });
  });

  return state;
}

function settings() {
  return {
    security: {
      session_idle_timeout_seconds: 1800,
      session_absolute_timeout_seconds: 28800,
      last_activity_persist_interval_seconds: 60,
      password_failures: {
        threshold: 8,
        window_seconds: 900,
        cooldown_seconds: 900
      },
      shared_account_failures: {
        threshold: 12,
        window_seconds: 900,
        cooldown_seconds: 900
      },
      entry_source_failures: {
        threshold: 20,
        window_seconds: 600,
        cooldown_seconds: 600
      }
    },
    storage: {
      media_root: null,
      warning_threshold_bytes: 107_374_182_400,
      media_write_stop_threshold_bytes: 34_359_738_368,
      trash_retention_days: 30
    },
    retry: { network_backoff_seconds: [60, 300, 1200, 3600] },
    queue: {
      quota_weights: {
        immediate: 4,
        manual_import: 8,
        scheduled_collection: 2,
        background_maintenance: 1
      },
      job_priorities: jobPriorities()
    },
    processing: {
      pixiv_request_concurrency: 4,
      pixiv_request_rate: { requests: 60, per_seconds: 60 },
      media_download_concurrency: 3,
      media_download_rate: { requests: 20, per_seconds: 60 },
      media_cpu_concurrency: 1
    },
    derivative: {
      format: 'webp',
      max_width: 768,
      webp_quality: 80,
      avif_quality: 50
    },
    pixiv: { default_private_bookmark: false },
    ugoira: {
      max_zip_bytes: 536_870_912,
      max_frames: 1500,
      max_pixels_per_frame: 80_000_000,
      decoded_frame_cache_bytes: 536_870_912
    },
    content: {
      overview_allow_nsfw: false,
      mask_non_all_age_thumbnails: false
    }
  };
}

function jobPriorities() {
  return [
    { job_kind: 'scheduled_collection', priority: 'scheduled_collection' },
    { job_kind: 'ranking_collection', priority: 'scheduled_collection' },
    { job_kind: 'following_collection', priority: 'scheduled_collection' },
    { job_kind: 'bookmarks_collection', priority: 'scheduled_collection' },
    { job_kind: 'import_artist', priority: 'manual_import' },
    { job_kind: 'import_work', priority: 'manual_import' },
    { job_kind: 'download_media', priority: 'background_maintenance' },
    {
      job_kind: 'generate_derivative',
      priority: 'background_maintenance'
    },
    { job_kind: 'purge_trash', priority: 'background_maintenance' }
  ];
}
