import { expect, test, type Page } from '@playwright/test';

import { fulfillJson, mockApi } from './support';

const accountId = '0198f650-0000-7000-8000-000000000001';
const ruleId = '0198f650-0000-7000-8000-000000000002';
const dailyId = '0198f650-0000-7000-8000-000000000011';
const weeklyId = '0198f650-0000-7000-8000-000000000012';
const followingId = '0198f650-0000-7000-8000-000000000013';

test.use({ timezoneId: 'Asia/Tokyo' });

test('subscriptions table edits schedules and exposes runs and cursors', async ({
  page
}) => {
  const api = await mockSubscriptions(page);
  await page.goto('/discovery/subscriptions');

  await expect(page.getByRole('heading', { name: '订阅计划' })).toBeVisible();
  await expect(page.getByText('每日综合榜')).toBeVisible();
  await expect(page.getByText('周榜插画')).toBeVisible();
  const nextRunTimes = page.locator('tbody time');
  await expect(nextRunTimes).toHaveCount(3);
  for (const time of await nextRunTimes.all()) {
    await expect(time).toHaveText(
      /^(?:今天|昨天|\d{1,2}月\d{1,2}日|\d{4}年\d{1,2}月\d{1,2}日) \d{2}:\d{2}$/
    );
  }

  await page.getByRole('button', { name: '查看订阅每日综合榜' }).click();
  const drawer = page.getByRole('region', { name: '订阅详情' });
  await expect(drawer.getByRole('region', { name: '采集对象' })).toContainText(
    '日榜'
  );
  await expect(drawer.getByRole('region', { name: '采集对象' })).toContainText(
    '前500名'
  );
  await expect(drawer.getByRole('region', { name: '应用规则' })).toContainText(
    '收藏数门槛'
  );
  await expect(drawer.getByRole('region', { name: '应用规则' })).toContainText(
    '收藏数 大于或等于 500'
  );
  await expect(drawer.getByText('日常游标', { exact: true })).toHaveCount(2);
  await expect(drawer.getByText('page 7')).toBeVisible();
  await expect(drawer.getByText('历史补采游标')).toBeVisible();
  await expect(drawer.getByText('page 32')).toBeVisible();

  await drawer.getByLabel('订阅名称').fill('每日综合与动图榜');
  await drawer.getByLabel('执行间隔（分钟）').fill('14');
  await drawer.getByRole('button', { name: '保存修改' }).click();
  await expect(
    drawer.getByText('执行间隔必须在15到43200分钟之间')
  ).toBeVisible();
  expect(api.lastUpdate).toBeUndefined();

  await drawer.getByLabel('执行间隔（分钟）').fill('720');
  await drawer.getByRole('button', { name: '保存修改' }).click();

  await expect
    .poll(() => api.lastUpdate)
    .toMatchObject({
      name: '每日综合与动图榜',
      interval_minutes: 720,
      expected_revision: 3
    });

  const runNowButton = drawer.getByRole('button', { name: '立即运行' });
  await expect(runNowButton).toBeEnabled();
  await runNowButton.click();
  await expect(drawer.getByText('已合并一次待运行')).toBeVisible();
  await expect.poll(() => api.runRequests).toEqual([{ backfill: false }]);

  await drawer.getByRole('button', { name: '查看运行记录' }).click();
  await expect(drawer.getByText('合并等待')).toBeVisible();
  await expect(drawer.getByText('发现 86')).toBeVisible();

  await drawer.getByRole('button', { name: '删除订阅' }).click();
  const confirmation = page.getByRole('dialog', { name: '删除订阅' });
  await expect(confirmation).toContainText('已经归档的作品不会受到影响');
  await confirmation.getByRole('button', { name: '删除订阅' }).click();
  await expect.poll(() => api.deleteRequests).toEqual([4]);
  await expect(page.getByText('每日综合与动图榜')).toHaveCount(0);
});

test('subscriptions keep fixed columns when empty and wrap long cursor data inside the drawer', async ({
  page
}) => {
  await mockSubscriptions(page);
  await page.goto('/discovery/subscriptions');

  await expect(page.locator('.subscription-table col')).toHaveCount(6);
  const populatedWidths = await page
    .locator('.subscription-table col')
    .evaluateAll((columns) =>
      columns.map((column) => getComputedStyle(column).width)
    );

  await page.getByLabel('订阅类型').click();
  await page.getByRole('option', { name: '收藏同步' }).click();
  await expect(page.getByText('没有符合当前筛选条件的订阅')).toBeVisible();
  const emptyWidths = await page
    .locator('.subscription-table col')
    .evaluateAll((columns) =>
      columns.map((column) => getComputedStyle(column).width)
    );
  expect(emptyWidths).toEqual(populatedWidths);

  await page.getByLabel('订阅类型').click();
  await page.getByRole('option', { name: '排行榜' }).click();
  await page.getByRole('button', { name: '查看订阅每日综合榜' }).click();
  const drawer = page.getByRole('region', { name: '订阅详情' });
  const overflow = await drawer.evaluate((element) => ({
    clientWidth: element.clientWidth,
    scrollWidth: element.scrollWidth,
    overflowing: [...element.querySelectorAll('*')].filter(
      (child) =>
        child.getBoundingClientRect().right >
        element.getBoundingClientRect().right + 1
    ).length
  }));
  expect(overflow.scrollWidth).toBeLessThanOrEqual(overflow.clientWidth + 1);
  expect(overflow.overflowing).toBe(0);
});

test('fixed following subscription keeps its definition read only and supports operations', async ({
  page
}) => {
  const api = await mockSubscriptions(page);
  await page.goto('/discovery/subscriptions');

  await expect(page.getByText('Pixiv关注动态')).toBeVisible();
  await page.getByLabel('订阅类型').click();
  await page.getByRole('option', { name: '关注作者' }).click();
  await expect(page.getByText('Pixiv关注动态')).toBeVisible();
  await expect(page.getByText('每日综合榜')).toHaveCount(0);

  await page.getByRole('button', { name: '查看订阅Pixiv关注动态' }).click();
  const drawer = page.getByRole('region', { name: '订阅详情' });
  const source = drawer.getByRole('region', { name: '采集对象' });
  await expect(source).toContainText('Pixiv关注动态');
  await expect(source).toContainText('全部作品');
  await expect(source).toContainText('每15分钟');
  await expect(source).not.toContainText('排行榜');
  await expect(drawer.getByLabel('订阅名称')).toHaveCount(0);
  await expect(drawer.getByRole('button', { name: '保存修改' })).toHaveCount(0);
  await expect(drawer.getByRole('button', { name: '删除订阅' })).toHaveCount(0);

  await drawer.getByRole('button', { name: '停用订阅' }).click();
  await expect
    .poll(() => api.enabledRequests)
    .toEqual([{ expected_revision: 2, enabled: false }]);
  await expect(drawer.getByText('订阅已经停用')).toBeVisible();
  await expect(drawer.getByRole('button', { name: '启用订阅' })).toBeVisible();

  await drawer.getByRole('button', { name: '立即运行' }).click();
  await expect.poll(() => api.followingRunRequests).toBe(1);
  await expect(drawer.getByText('已经加入定时采集队列')).toBeVisible();
});

test('invalid account is explained without a subscription waiting state', async ({
  page
}) => {
  await mockSubscriptions(page, { accountState: 'credential_invalid' });
  await page.goto('/discovery/subscriptions');

  const row = page.getByRole('button', { name: '查看订阅每日综合榜' });
  await expect(row).toContainText('等待账户恢复');
  await row.click();

  const drawer = page.getByRole('region', { name: '订阅详情' });
  await expect(drawer).toContainText('Cookie已经失效');
  await expect(drawer).toContainText('更新并验证Cookie后会自动继续');
  await expect(drawer.getByRole('button', { name: '立即运行' })).toBeDisabled();
});

test('restricted account warning keeps eligible subscription runs available', async ({
  page
}) => {
  await mockSubscriptions(page, { accountState: 'restricted' });
  await page.goto('/discovery/subscriptions');

  const row = page.getByRole('button', { name: '查看订阅周榜插画' });
  await row.click();

  const drawer = page.getByRole('region', { name: '订阅详情' });
  await expect(drawer).toContainText('当前账户无法访问R-18内容');
  await expect(drawer.getByRole('button', { name: '立即运行' })).toBeEnabled();
});

test('subscription and rule lists can be retried after loading fails', async ({
  page
}) => {
  const api = await mockSubscriptions(page, {
    blockSubscriptionLoads: true,
    blockRuleLoads: true
  });
  await page.goto('/discovery/subscriptions');

  await expect(page.getByText('订阅列表暂时无法读取')).toBeVisible();
  await expect(page.getByText('规则列表暂时无法读取')).toBeVisible();

  api.allowSubscriptionLoads();
  await page.getByRole('button', { name: '重新读取订阅列表' }).click();
  await expect(page.getByText('每日综合榜')).toBeVisible();
  await expect(page.getByText('订阅列表暂时无法读取')).toHaveCount(0);

  api.allowRuleLoads();
  await page.getByRole('button', { name: '重新读取规则列表' }).click();
  await expect(page.getByText('规则列表暂时无法读取')).toHaveCount(0);
});

test('subscription ownership, enabled state and active run stay in sync', async ({
  page
}) => {
  const api = await mockSubscriptions(page);
  await page.goto('/discovery/subscriptions');

  const row = page.getByRole('button', { name: '查看订阅每日综合榜' });
  await expect(row).toContainText('Pixiv ID 10001');
  await expect(row.locator('img')).toHaveAttribute(
    'src',
    `/api/pixiv/accounts/${accountId}/avatar?revision=2`
  );
  await row.click();

  const drawer = page.getByRole('region', { name: '订阅详情' });
  await drawer.getByRole('button', { name: '停止本次运行' }).click();
  await expect.poll(() => api.stopRequests).toBe(1);
  await expect(drawer.getByText('本次运行已经停止')).toBeVisible();
  await expect(
    drawer.getByRole('button', { name: '停止本次运行' })
  ).toHaveCount(0);
  await expect(row).toContainText('已暂停');

  await drawer.getByRole('button', { name: '停用订阅' }).click();
  await expect
    .poll(() => api.enabledRequests)
    .toEqual([{ expected_revision: 4, enabled: false }]);
  await expect(drawer.getByText('订阅已经停用')).toBeVisible();
  await expect(row).toContainText('已经停用');
  await expect(drawer.getByRole('button', { name: '启用订阅' })).toBeVisible();
});

test('rankings creates one subscription with several ranking modes', async ({
  page
}) => {
  const api = await mockSubscriptions(page);
  await page.goto('/discovery/rankings');
  await expect(
    page.getByRole('button', { name: '重新读取规则列表' })
  ).toHaveCount(0);

  await expect(page.getByRole('heading', { name: '排行榜订阅' })).toBeVisible();
  await page.getByLabel('订阅名称').fill('每日与R-18榜');
  await page.getByLabel('日榜').check();
  await page.getByLabel('R-18榜').check();
  await page.getByLabel('男性向').check();
  await page.getByLabel('女性向').check();
  await page.getByLabel('综合').check();
  await page.getByLabel('插画').check();
  await page.getByLabel('动图').check();
  await page.getByLabel('每个榜单采集前多少名').fill('500');
  await page.getByLabel('补采最近多少期').fill('8');
  await page.getByRole('button', { name: '创建排行榜订阅' }).click();
  await expect(page.getByText('补采最近多少期必须在0到7之间')).toBeVisible();
  expect(api.lastCreate).toBeUndefined();

  await page.getByLabel('补采最近多少期').fill('3');
  await expect(page.getByLabel('规则')).toContainText('不应用下载规则');
  await expect(page.getByText('4000')).toBeVisible();
  await page.getByRole('button', { name: '创建排行榜订阅' }).click();

  await expect
    .poll(() => api.lastCreate)
    .toMatchObject({
      kind: 'ranking',
      name: '每日与R-18榜',
      account_id: accountId,
      rule_id: null,
      lookback_pages: 3,
      params: {
        modes: ['daily', 'r18', 'male', 'female'],
        contents: ['all', 'illustration', 'ugoira'],
        max_rank: 500
      }
    });
  await expect(page.getByText('已建立排行榜订阅')).toBeVisible();
});

test('rankings keeps loaded rules while a failed refresh is retried', async ({
  page
}) => {
  const api = await mockSubscriptions(page);
  await page.goto('/discovery/rankings');

  await page.getByLabel('规则').click();
  await expect(
    page.getByRole('option', { name: '默认下载规则' })
  ).toBeVisible();
  await page.keyboard.press('Escape');

  api.blockRuleLoads();
  await page.evaluate(() =>
    document.dispatchEvent(new Event('visibilitychange'))
  );
  await expect(page.getByText('规则列表暂时无法读取')).toBeVisible();
  await page.getByLabel('规则').click();
  await expect(
    page.getByRole('option', { name: '默认下载规则' })
  ).toBeVisible();
  await page.getByRole('option', { name: '默认下载规则' }).click();

  api.allowRuleLoads();
  await page.getByRole('button', { name: '重新读取规则列表' }).click();
  await expect(page.getByText('规则列表暂时无法读取')).toHaveCount(0);
  await expect(
    page.getByRole('button', { name: '重新读取规则列表' })
  ).toHaveCount(0);
});

interface SubscriptionMockState {
  lastCreate?: Record<string, unknown>;
  lastUpdate?: Record<string, unknown>;
  runRequests: Array<Record<string, unknown>>;
  deleteRequests: number[];
  followingRunRequests: number;
  stopRequests: number;
  enabledRequests: Array<Record<string, unknown>>;
  allowSubscriptionLoads: () => void;
  blockRuleLoads: () => void;
  allowRuleLoads: () => void;
}

interface SubscriptionMockOptions {
  blockSubscriptionLoads?: boolean;
  blockRuleLoads?: boolean;
  accountState?: PixivAccountState;
}

type PixivAccountState =
  | 'unconfigured'
  | 'validating'
  | 'normal'
  | 'restricted'
  | 'credential_invalid';

async function mockSubscriptions(
  page: Page,
  options: SubscriptionMockOptions = {}
): Promise<SubscriptionMockState> {
  await mockApi(page);
  let subscriptionLoadsBlocked = options.blockSubscriptionLoads ?? false;
  let ruleLoadsBlocked = options.blockRuleLoads ?? false;
  const accountState = options.accountState ?? 'normal';
  const state: SubscriptionMockState = {
    runRequests: [],
    deleteRequests: [],
    followingRunRequests: 0,
    stopRequests: 0,
    enabledRequests: [],
    allowSubscriptionLoads: () => (subscriptionLoadsBlocked = false),
    blockRuleLoads: () => (ruleLoadsBlocked = true),
    allowRuleLoads: () => (ruleLoadsBlocked = false)
  };
  const subscriptions = [
    subscription(
      dailyId,
      '每日综合榜',
      ['daily'],
      ['all'],
      true,
      true,
      3,
      accountState
    ),
    subscription(
      weeklyId,
      '周榜插画',
      ['weekly'],
      ['illustration'],
      true,
      false,
      1,
      accountState
    ),
    followingSubscription(accountState)
  ];

  await page.route('**/api/subscriptions', async (route) => {
    if (route.request().method() === 'GET') {
      if (subscriptionLoadsBlocked) {
        await fulfillJson(route, 503, { error: 'unavailable' });
        return;
      }
      await fulfillJson(route, 200, { items: subscriptions });
      return;
    }
    const request = route.request().postDataJSON() as Record<string, unknown>;
    state.lastCreate = request;
    const created = subscription(
      '0198f650-0000-7000-8000-000000000099',
      String(request.name),
      ['daily', 'r18'],
      ['illustration', 'ugoira'],
      true,
      false,
      1,
      accountState
    );
    subscriptions.push(created);
    await fulfillJson(route, 201, created);
  });

  await page.route(`**/api/subscriptions/${dailyId}`, async (route) => {
    if (route.request().method() === 'GET') {
      await fulfillJson(route, 200, subscriptions[0]);
      return;
    }
    const request = route.request().postDataJSON() as Record<string, unknown>;
    state.lastUpdate = request;
    Object.assign(subscriptions[0], {
      ...request,
      schedule: {
        interval_minutes: request.interval_minutes,
        lookback_pages: request.lookback_pages
      },
      revision: 4
    });
    await fulfillJson(route, 200, subscriptions[0]);
  });

  await page.route(
    `**/api/subscriptions/${dailyId}?expected_revision=*`,
    async (route) => {
      state.deleteRequests.push(
        Number(
          new URL(route.request().url()).searchParams.get('expected_revision')
        )
      );
      subscriptions.splice(0, 1);
      await route.fulfill({ status: 204, body: '' });
    }
  );

  await page.route(`**/api/subscriptions/${dailyId}/run`, async (route) => {
    state.runRequests.push(route.request().postDataJSON());
    subscriptions[0].pending_run = true;
    await fulfillJson(route, 202, {
      subscription_id: dailyId,
      run_id: '0198f650-0000-7000-8000-000000000021',
      job_id: '0198f650-0000-7000-8000-000000000022',
      trigger_kind: 'merged_pending'
    });
  });

  await page.route(`**/api/subscriptions/${dailyId}/stop`, async (route) => {
    state.stopRequests += 1;
    Object.assign(subscriptions[0], {
      pending_run: false,
      recent_state: 'paused',
      revision: 4
    });
    await fulfillJson(route, 200, subscriptions[0]);
  });

  await page.route(`**/api/subscriptions/${dailyId}/enabled`, async (route) => {
    const request = route.request().postDataJSON() as Record<string, unknown>;
    state.enabledRequests.push(request);
    Object.assign(subscriptions[0], {
      enabled: request.enabled,
      revision: 5
    });
    await fulfillJson(route, 200, subscriptions[0]);
  });

  await page.route(`**/api/subscriptions/${followingId}/run`, async (route) => {
    state.followingRunRequests += 1;
    await fulfillJson(route, 202, {
      subscription_id: followingId,
      run_id: '0198f650-0000-7000-8000-000000000041',
      job_id: '0198f650-0000-7000-8000-000000000042',
      trigger_kind: 'manual'
    });
  });

  await page.route(
    `**/api/subscriptions/${followingId}/enabled`,
    async (route) => {
      const request = route.request().postDataJSON() as Record<string, unknown>;
      state.enabledRequests.push(request);
      Object.assign(subscriptions[2], {
        enabled: request.enabled,
        revision: 3
      });
      await fulfillJson(route, 200, subscriptions[2]);
    }
  );

  await page.route(`**/api/subscriptions/${followingId}`, async (route) => {
    await fulfillJson(route, 200, subscriptions[2]);
  });

  await page.route(`**/api/subscriptions/${dailyId}/runs**`, async (route) => {
    await fulfillJson(route, 200, {
      items: [
        {
          id: '0198f650-0000-7000-8000-000000000031',
          subscription_id: dailyId,
          trigger_kind: 'merged_pending',
          state: 'succeeded',
          cursor_kind: 'incremental',
          discovered_count: 86,
          ignored_count: 24,
          error_class: null,
          trace_id: '0198f650-0000-7000-8000-000000000032',
          started_at: '2026-07-30T03:00:00Z',
          finished_at: '2026-07-30T03:04:00Z',
          created_at: '2026-07-30T03:00:00Z'
        }
      ]
    });
  });

  await page.route(`**/api/subscriptions/${dailyId}/cursors`, async (route) => {
    await fulfillJson(route, 200, {
      items: [
        {
          cursor_kind: 'incremental',
          source_key: 'daily:all',
          value: { page: 7 }
        },
        {
          cursor_kind: 'backfill',
          source_key: 'daily:all',
          value: { page: 32 }
        },
        {
          cursor_kind: 'incremental',
          source_key:
            'dailybookmarksyncpublicprivateverylongsourcekey012345678901234567890123456789',
          value: {
            offset:
              'ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789'
          }
        }
      ]
    });
  });

  await page.route(
    `**/api/subscriptions/${followingId}/runs**`,
    async (route) => {
      await fulfillJson(route, 200, { items: [] });
    }
  );

  await page.route(
    `**/api/subscriptions/${followingId}/cursors`,
    async (route) => {
      await fulfillJson(route, 200, { items: [] });
    }
  );

  await page.route('**/api/rules', async (route) => {
    if (ruleLoadsBlocked) {
      await fulfillJson(route, 503, { error: 'unavailable' });
      return;
    }
    await fulfillJson(route, 200, {
      items: [
        {
          id: ruleId,
          name: '默认下载规则',
          enabled: true,
          action: 'download',
          default_action: 'download',
          current_version_id: '0198f650-0000-7000-8000-000000000003',
          current_version: 1,
          lifecycle: 'published',
          revision: 1
        }
      ]
    });
  });

  await page.route(`**/api/rules/${ruleId}/export`, async (route) => {
    await fulfillJson(route, 200, {
      schema_version: 1,
      id: ruleId,
      name: '收藏数门槛',
      enabled: true,
      group_mode: 'all',
      groups: [
        {
          mode: 'all',
          conditions: [
            {
              field: 'bookmark_count',
              operator: 'greater_than_or_equal',
              value: { type: 'number', value: 500 }
            }
          ]
        }
      ],
      action: 'download',
      default_action: 'download'
    });
  });

  await page.route('**/api/pixiv/account', async (route) => {
    await fulfillJson(route, 200, {
      account_id: accountId,
      pixiv_user_id: 10001,
      display_name: 'Test Artist',
      state: accountState,
      bookmark_writeback_enabled: false,
      last_validated_at: '2026-07-30T02:00:00Z',
      revision: 2
    });
  });

  await page.route(
    `**/api/pixiv/accounts/${accountId}/avatar?revision=2`,
    async (route) => {
      await route.fulfill({
        status: 200,
        contentType: 'image/svg+xml',
        body: '<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32" />'
      });
    }
  );

  return state;
}

function subscription(
  id: string,
  name: string,
  modes: string[],
  contents: string[],
  enabled: boolean,
  pending: boolean,
  revision: number,
  accountState: PixivAccountState = 'normal'
) {
  return {
    id,
    account_id: accountId,
    account_pixiv_user_id: 10001,
    account_avatar_url: `/api/pixiv/accounts/${accountId}/avatar?revision=2`,
    account_state: accountState,
    rule_id: ruleId,
    name,
    kind: 'ranking',
    enabled,
    schedule: { interval_minutes: 1440, lookback_pages: 2 },
    params: { modes, contents, max_rank: 500 },
    next_run_at: '2026-07-31T02:00:00Z',
    pending_run: pending,
    recent_state: pending ? 'running' : 'succeeded',
    revision
  };
}

function followingSubscription(accountState: PixivAccountState = 'normal') {
  return {
    id: followingId,
    account_id: accountId,
    account_pixiv_user_id: 10001,
    account_avatar_url: `/api/pixiv/accounts/${accountId}/avatar?revision=2`,
    account_state: accountState,
    rule_id: null,
    name: 'Pixiv关注动态',
    kind: 'following',
    enabled: true,
    schedule: { interval_minutes: 15, lookback_pages: 2 },
    params: { source: 'following', mode: 'all' },
    next_run_at: '2026-07-31T02:00:00Z',
    pending_run: false,
    recent_state: 'succeeded',
    revision: 2
  };
}
