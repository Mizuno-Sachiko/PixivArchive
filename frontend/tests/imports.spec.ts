import { expect, test, type Page } from '@playwright/test';

import type { ImportRun } from '$lib/api/imports';

import {
  chooseSelectOption,
  fulfillJson,
  mockApi,
  mockPixivAccount
} from './support';

test('imports keeps artist and work runs outside recurring subscriptions', async ({
  page
}) => {
  const api = await mockImports(page);
  await page.goto('/discovery/imports');

  await expect(page.getByRole('heading', { name: '手动导入' })).toBeVisible();
  await expect.poll(() => api.listLimit).toBe('200');
  await expect(page.getByText('最近1项')).toHaveAttribute(
    'title',
    '列表最多显示最近200项'
  );
  for (const heading of [
    '目标',
    '采集方式',
    '状态',
    '发现 / 保存',
    '建立时间'
  ]) {
    await expect(
      page.getByRole('columnheader', { name: heading })
    ).toBeVisible();
  }
  const initialRun = page.getByRole('row').filter({ hasText: '作品 190016' });
  await expect(initialRun.getByText('默认下载', { exact: true })).toBeVisible();
  await page.getByRole('tab', { name: '作者ID' }).click();
  await page.getByLabel('Pixiv ID').fill('190013');
  await page.getByRole('button', { name: '建立导入任务' }).click();
  await expect
    .poll(() => api.lastRequest)
    .toEqual({
      account_id: '0198f651-0000-7000-8000-000000000001',
      kind: 'artist',
      target_pixiv_id: 190013,
      strategy: { mode: 'default' }
    });

  await page.getByLabel('Pixiv ID').fill('190014');
  await chooseSelectOption(page, '采集方式', '按规则采集');
  await chooseSelectOption(page, '下载规则', '收藏数门槛 · v3');
  await page.getByRole('button', { name: '建立导入任务' }).click();

  await expect
    .poll(() => api.lastRequest)
    .toEqual({
      account_id: '0198f651-0000-7000-8000-000000000001',
      kind: 'artist',
      target_pixiv_id: 190014,
      strategy: { mode: 'rule', rule_id: RULE_ID }
    });
  await expect(page.getByText('作者 190014')).toBeVisible();
  await expect(page.getByText('正在等待')).toBeVisible();

  await page.getByRole('tab', { name: '作品ID' }).click();
  await page.getByLabel('Pixiv ID').fill('190015');
  await chooseSelectOption(page, '采集方式', '强制下载');
  await page.getByRole('button', { name: '建立导入任务' }).click();
  await expect
    .poll(() => api.lastRequest)
    .toEqual({
      account_id: '0198f651-0000-7000-8000-000000000001',
      kind: 'work',
      target_pixiv_id: 190015,
      strategy: { mode: 'forced' }
    });
  const forcedRun = page.getByRole('row').filter({ hasText: '作品 190015' });
  await expect(forcedRun.getByText('强制下载', { exact: true })).toBeVisible();
});

test('imports pauses account-bound commands while the current account changes', async ({
  page
}) => {
  const api = await mockImports(page);
  await page.unroute('**/api/pixiv/account');
  await page.unroute('**/api/events');

  const accountA = mockPixivAccount({
    account_id: '0198f651-0000-7000-8000-000000000001',
    pixiv_user_id: 10001,
    display_name: 'Account A'
  });
  const accountB = mockPixivAccount({
    account_id: '0198f651-0000-7000-8000-000000000002',
    pixiv_user_id: 10002,
    display_name: 'Account B'
  });
  let currentAccount = accountA;
  let accountRequests = 0;
  let announceAccountRefresh!: () => void;
  const accountRefreshStarted = new Promise<void>((resolve) => {
    announceAccountRefresh = resolve;
  });
  let releaseAccountRefresh!: () => void;
  const accountRefreshGate = new Promise<void>((resolve) => {
    releaseAccountRefresh = resolve;
  });
  let publishAccountChange!: () => void;
  const accountChange = new Promise<void>((resolve) => {
    publishAccountChange = resolve;
  });
  let eventConnections = 0;

  await page.route('**/api/pixiv/account', async (route) => {
    accountRequests += 1;
    if (accountRequests > 1) {
      announceAccountRefresh();
      await accountRefreshGate;
    }
    await fulfillJson(route, 200, currentAccount);
  });
  await page.route('**/api/events', async (route) => {
    eventConnections += 1;
    if (eventConnections > 1) {
      await route.fulfill({ status: 204 });
      return;
    }
    await accountChange;
    await route.fulfill({
      status: 200,
      headers: { 'Content-Type': 'text/event-stream' },
      body: `event: app_event\ndata: {"resource":"pixiv_account","resource_id":"${accountB.account_id}"}\n\n`
    });
  });

  await page.goto('/discovery/imports');
  await page.getByRole('tab', { name: '作者ID' }).click();
  await page.getByLabel('Pixiv ID').fill('190017');
  const createButton = page.getByRole('button', { name: '建立导入任务' });
  await expect(createButton).toBeEnabled();

  currentAccount = accountB;
  publishAccountChange();
  await accountRefreshStarted;
  await expect(createButton).toBeDisabled();

  releaseAccountRefresh();
  await expect(createButton).toBeEnabled();
  await createButton.click();
  await expect
    .poll(() => api.lastRequest?.account_id)
    .toBe(accountB.account_id);
});

test('imports keeps history visible while its page data is retried', async ({
  page
}) => {
  const api = await mockImports(page);
  await page.goto('/discovery/imports');
  await expect(page.getByText('作品 190016')).toBeVisible();
  await expect(
    page.getByRole('button', { name: '重新读取导入记录和规则' })
  ).toHaveCount(0);

  api.blockLoads();
  await page.evaluate(() =>
    document.dispatchEvent(new Event('visibilitychange'))
  );
  await expect(page.getByText('导入记录或规则暂时无法读取')).toBeVisible();
  await expect(page.getByText('作品 190016')).toBeVisible();

  api.allowLoads();
  await page.getByRole('button', { name: '重新读取导入记录和规则' }).click();
  await expect(page.getByText('导入记录或规则暂时无法读取')).toHaveCount(0);
  await expect(
    page.getByRole('button', { name: '重新读取导入记录和规则' })
  ).toHaveCount(0);
  await expect(page.getByText('作品 190016')).toBeVisible();
});

const RULE_ID = '0198f650-0000-7000-8000-000000000003';

interface ImportMockState {
  lastRequest?: Record<string, unknown>;
  listLimit?: string | null;
  blockLoads: () => void;
  allowLoads: () => void;
}

async function mockImports(page: Page): Promise<ImportMockState> {
  await mockApi(page);
  let loadsBlocked = false;
  const state: ImportMockState = {
    blockLoads: () => (loadsBlocked = true),
    allowLoads: () => (loadsBlocked = false)
  };
  const history: ImportRun[] = [
    {
      id: '0198f651-0000-7000-8000-000000000021',
      job_id: '0198f651-0000-7000-8000-000000000022',
      account_id: '0198f651-0000-7000-8000-000000000001',
      kind: 'work',
      target_pixiv_id: 190016,
      strategy: { mode: 'default' },
      status: 'download_queued',
      discovered_count: 1,
      saved_count: 1,
      error_class: null,
      error_message: null,
      created_at: '2026-07-30T01:00:00Z',
      finished_at: '2026-07-30T01:00:03Z'
    }
  ];

  await page.route(/\/api\/imports(?:\?.*)?$/, async (route) => {
    if (route.request().method() === 'GET') {
      state.listLimit = new URL(route.request().url()).searchParams.get(
        'limit'
      );
      if (loadsBlocked) {
        await fulfillJson(route, 503, { error: 'unavailable' });
        return;
      }
      await fulfillJson(route, 200, { items: history });
      return;
    }
    const request = route.request().postDataJSON() as {
      account_id: string;
      kind: 'artist' | 'work';
      target_pixiv_id: number;
      strategy:
        { mode: 'default' | 'forced' } | { mode: 'rule'; rule_id: string };
    };
    state.lastRequest = request;
    const run = {
      id: '0198f651-0000-7000-8000-000000000031',
      job_id: '0198f651-0000-7000-8000-000000000032',
      account_id: request.account_id,
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
    history.unshift(run);
    await fulfillJson(route, 202, run);
  });

  await page.route('**/api/pixiv/account', async (route) => {
    await fulfillJson(route, 200, {
      account_id: '0198f651-0000-7000-8000-000000000001',
      pixiv_user_id: 10001,
      display_name: 'Test Artist',
      state: 'normal',
      bookmark_writeback_enabled: false,
      last_validated_at: '2026-07-30T02:00:00Z',
      revision: 2
    });
  });

  await page.route('**/api/rules', async (route) => {
    await fulfillJson(route, 200, {
      items: [
        {
          id: RULE_ID,
          name: '收藏数门槛',
          enabled: true,
          action: 'download',
          default_action: 'ignore',
          current_version_id: '0198f650-0000-7000-8000-000000000004',
          current_version: 3,
          lifecycle: 'published',
          revision: 4,
          sort_order: 0
        }
      ]
    });
  });

  return state;
}
