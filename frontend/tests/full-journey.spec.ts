import { expect, test, type Page } from '@playwright/test';

import { fulfillJson, mockApi } from './support';

const accountId = '0198f660-0000-7000-8000-000000000001';
const ruleId = '0198f660-0000-7000-8000-000000000002';
const workId = '0198f660-0000-7000-8000-000000000003';

test('full journey keeps account discovery archive and system surfaces connected', async ({
  page
}) => {
  const state = await mockJourney(page);

  await page.goto('/login');
  await page.getByLabel('管理员密码').fill('local-test-password');
  await page.getByRole('button', { name: '登录' }).click();
  await expect(page).toHaveURL(/\/overview$/);
  await expect.poll(() => state.events.eventConnections).toBeGreaterThan(0);

  await page.goto('/system/account');
  await expect(page.getByText('Test Artist')).toBeVisible();
  await page
    .getByLabel('Pixiv Cookie')
    .fill('PHPSESSID=10001_fixture-session-value');
  await page.getByRole('button', { name: '保存并验证' }).click();
  await expect(page.getByText('验证成功')).toBeVisible();
  expect(state.accountUpdate).toEqual({
    cookie: 'PHPSESSID=10001_fixture-session-value'
  });

  await page.goto('/discovery/rankings');
  await page.getByLabel('订阅名称').fill('每日插画榜');
  await page.getByLabel('日榜').check();
  await page.getByLabel('插画').check();
  await page.getByRole('button', { name: '创建排行榜订阅' }).click();
  await expect(page.getByText('已建立排行榜订阅')).toBeVisible();
  expect(state.subscriptionCreate).toMatchObject({
    kind: 'ranking',
    account_id: accountId,
    rule_id: null,
    name: '每日插画榜'
  });

  await page.goto('/discovery/imports');
  await page.getByRole('tab', { name: '作品ID' }).click();
  await page.getByLabel('Pixiv ID').fill('190015');
  await page.getByRole('button', { name: '建立导入任务' }).click();
  await expect(page.getByText('作品 190015')).toBeVisible();
  expect(state.importCreate).toMatchObject({
    account_id: accountId,
    kind: 'work',
    target_pixiv_id: 190015,
    strategy: { mode: 'default' }
  });

  await page.goto('/gallery');
  await expect(page.getByText('夏夜の星')).toBeVisible();
  await expect(page.getByText('动图', { exact: true })).toBeVisible();

  await page.goto('/system/trash');
  await expect(page.getByRole('heading', { name: '待清理作品' })).toBeVisible();
  await expect(page.getByRole('heading', { name: '轻量删除标记' })).toHaveCount(
    0
  );
  await expect(page.getByText('190016')).toHaveCount(0);

  await page.goto('/system/about');
  await expect(page.getByText('v0.1.0')).toBeVisible();
});

interface JourneyState {
  events: Awaited<ReturnType<typeof mockApi>>;
  accountUpdate?: Record<string, unknown>;
  subscriptionCreate?: Record<string, unknown>;
  importCreate?: Record<string, unknown>;
}

async function mockJourney(page: Page): Promise<JourneyState> {
  const events = await mockApi(page, false);
  const state: JourneyState = { events };
  const imports: Array<Record<string, unknown>> = [];

  await page.route('**/api/pixiv/account', async (route) => {
    if (route.request().method() === 'PUT') {
      state.accountUpdate = route.request().postDataJSON();
    }
    await fulfillJson(route, 200, {
      account_id: accountId,
      pixiv_user_id: 10001,
      display_name: 'Test Artist',
      state: 'normal',
      bookmark_writeback_enabled: false,
      last_validated_at: '2026-07-30T02:00:00Z'
    });
  });

  await page.route('**/api/rules', async (route) => {
    await fulfillJson(route, 200, {
      items: [
        {
          id: ruleId,
          name: '默认下载规则',
          default_action: 'ignore',
          current_version_id: null,
          current_version: null,
          revision: 1
        }
      ]
    });
  });

  await page.route('**/api/subscriptions', async (route) => {
    if (route.request().method() === 'GET') {
      await fulfillJson(route, 200, { items: [] });
      return;
    }
    state.subscriptionCreate = route.request().postDataJSON();
    await fulfillJson(route, 201, {
      id: '0198f660-0000-7000-8000-000000000010',
      account_id: accountId,
      rule_id: ruleId,
      name: '每日插画榜',
      kind: 'ranking',
      enabled: true,
      schedule: { interval_minutes: 1440, lookback_pages: 1 },
      params: { modes: ['daily'], contents: ['illustration'], max_rank: 500 },
      next_run_at: '2026-07-31T02:00:00Z',
      pending_run: false,
      recent_state: null,
      revision: 1
    });
  });

  await page.route(/\/api\/imports(?:\?.*)?$/, async (route) => {
    if (route.request().method() === 'GET') {
      await fulfillJson(route, 200, { items: imports });
      return;
    }
    const request = route.request().postDataJSON() as {
      account_id: string;
      kind: 'artist' | 'work';
      target_pixiv_id: number;
      strategy:
        { mode: 'default' | 'forced' } | { mode: 'rule'; rule_id: string };
    };
    state.importCreate = request;
    const run = {
      id: '0198f660-0000-7000-8000-000000000020',
      job_id: '0198f660-0000-7000-8000-000000000021',
      account_id: accountId,
      kind: request.kind,
      target_pixiv_id: request.target_pixiv_id,
      strategy: request.strategy,
      status: 'queued',
      discovered_count: 0,
      saved_count: 0,
      error_class: null,
      error_message: null,
      created_at: '2026-07-30T04:00:00Z',
      finished_at: null
    };
    imports.unshift(run);
    await fulfillJson(route, 202, run);
  });

  await page.route('**/api/gallery/search', async (route) => {
    await fulfillJson(route, 200, {
      items: [
        {
          id: workId,
          pixiv_work_id: 190015,
          title: '夏夜の星',
          description: '',
          artist_id: '0198f660-0000-7000-8000-000000000004',
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
          cover_url: '/api/derivatives/0198f660-0000-7000-8000-000000000005',
          cover_width: 640,
          cover_height: 900,
          media_kind: 'ugoira_zip',
          tags: []
        }
      ],
      next_cursor: null
    });
  });

  await page.route('**/api/derivatives/*', async (route) => {
    await route.fulfill({
      status: 200,
      contentType: 'image/svg+xml',
      body: '<svg xmlns="http://www.w3.org/2000/svg" width="640" height="900"><rect width="640" height="900" fill="#0096fa"/></svg>'
    });
  });

  await page.route('**/api/trash?*', async (route) => {
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

  return state;
}
