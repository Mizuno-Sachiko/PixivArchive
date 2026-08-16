import { expect, test, type Page } from '@playwright/test';

import { fulfillJson, mockApi, PIXIV_ACCOUNT_ID } from './support';

const subscriptionId = '0198f652-0000-7000-8000-000000000001';

test.use({ timezoneId: 'Asia/Tokyo' });

test('following manages the fixed subscription and selected artists', async ({
  page
}) => {
  await page.clock.setFixedTime(new Date('2026-07-31T14:00:00Z'));
  const api = await mockFollowing(page);
  await page.goto('/discovery/following');

  await expect(
    page.getByRole('heading', { name: '关注订阅', exact: true })
  ).toBeVisible();
  await expect(page.getByText('Artist Alpha')).toBeVisible();
  await expect(page.getByText('Artist Beta')).toBeVisible();
  await expect(page.getByText('今天 21:30')).toBeVisible();
  await expect(page.getByText('尚未抓取')).toBeVisible();
  await expect(page.getByText('上次完整核对：今天 18:53')).toBeVisible();
  const interval = page.getByRole('button', {
    name: '关注自动同步间隔'
  });
  await expect(interval).toContainText('15分钟');
  await interval.click();
  await page.getByRole('option', { name: '30分钟' }).click();
  await expect
    .poll(() => api.subscriptionUpdates)
    .toEqual([
      {
        expected_account_id: PIXIV_ACCOUNT_ID,
        enabled: true,
        interval_minutes: 30,
        expected_revision: 3
      }
    ]);
  await expect(
    page.getByRole('link', { name: '在Pixiv打开Artist Alpha' })
  ).toHaveAttribute('href', 'https://www.pixiv.net/users/101');
  await expect(
    page.getByRole('link', { name: '在Pixiv打开Artist Alpha' })
  ).toHaveAttribute('target', '_blank');
  const alphaRow = page.locator('.author-row').filter({
    hasText: 'Artist Alpha'
  });
  await expect(alphaRow.locator('.author-time')).toContainText('今天 21:30');
  await expect(
    alphaRow.locator('.author-time').locator('a + .author-collected-time time')
  ).toBeVisible();

  await page.getByRole('switch', { name: '启用关注订阅' }).uncheck();
  await expect
    .poll(() => api.subscriptionUpdates)
    .toEqual([
      {
        expected_account_id: PIXIV_ACCOUNT_ID,
        enabled: true,
        interval_minutes: 30,
        expected_revision: 3
      },
      {
        expected_account_id: PIXIV_ACCOUNT_ID,
        enabled: false,
        interval_minutes: 30,
        expected_revision: 4
      }
    ]);

  await page.getByRole('checkbox', { name: '采集Artist Beta' }).uncheck();
  await expect
    .poll(() => api.authorUpdates)
    .toEqual([
      {
        pixivArtistId: 102,
        expectedAccountId: PIXIV_ACCOUNT_ID,
        enabled: false
      }
    ]);

  await page.getByRole('button', { name: '刷新关注列表' }).click();
  await expect.poll(() => api.refreshRequests).toEqual([PIXIV_ACCOUNT_ID]);
  await expect(page.getByText('Artist Gamma')).toBeVisible();

  await page.getByRole('button', { name: '立即运行' }).click();
  await expect
    .poll(() => api.runRequests)
    .toEqual([{ expectedAccountId: PIXIV_ACCOUNT_ID, backfill: false }]);
  await expect(page.getByText('关注采集任务已加入队列')).toBeVisible();

  await page.getByRole('button', { name: '完整同步' }).click();
  await expect
    .poll(() => api.runRequests)
    .toEqual([
      { expectedAccountId: PIXIV_ACCOUNT_ID, backfill: false },
      { expectedAccountId: PIXIV_ACCOUNT_ID, backfill: true }
    ]);
  await expect(page.getByText('关注完整同步任务已加入队列')).toBeVisible();
});

test('following bulk collection keeps row controls usable', async ({
  page
}) => {
  const api = await mockFollowing(page);
  await page.goto('/discovery/following');

  await page.getByRole('button', { name: '多选' }).click();
  const alphaRow = page.locator('.author-row').filter({
    hasText: 'Artist Alpha'
  });
  const betaRow = page.locator('.author-row').filter({
    hasText: 'Artist Beta'
  });
  await alphaRow.locator('.visibility').click();
  await expect(
    alphaRow.getByRole('checkbox', { name: '选择Artist Alpha' })
  ).toBeChecked();

  await betaRow.getByRole('checkbox', { name: '采集Artist Beta' }).uncheck();
  await expect(
    betaRow.getByRole('checkbox', { name: '选择Artist Beta' })
  ).not.toBeChecked();
  await expect(page.getByRole('button', { name: '全选' })).toBeEnabled();

  await page.getByRole('button', { name: '全选' }).click();
  await page.getByRole('button', { name: '停止采集' }).click();
  await expect
    .poll(() => api.authorBatchUpdates)
    .toEqual([
      {
        expectedAccountId: PIXIV_ACCOUNT_ID,
        pixivArtistIds: [101, 102],
        enabled: false
      }
    ]);
  await expect(page.getByText('所选作者已停止采集')).toBeVisible();
  await expect(
    page.getByRole('link', { name: '在Pixiv打开Artist Alpha' })
  ).toBeEnabled();
});

test('following avatar retries when the saved source changes', async ({
  page
}) => {
  await mockApi(page);
  const missingAvatar = 'https://i.pximg.net/avatar-missing.svg';
  const recoveredAvatar = 'https://i.pximg.net/avatar-recovered.svg';
  const subscription = fixedSubscription(true, 3);
  let avatarUrl = missingAvatar;

  await page.route(/\/api\/following$/, async (route) => {
    await fulfillJson(route, 200, {
      subscription,
      authors: [author(101, 'Artist Alpha', 'public', true, null, avatarUrl)]
    });
  });
  await page.route('**/api/following/refresh', async (route) => {
    avatarUrl = recoveredAvatar;
    await fulfillJson(route, 200, {
      subscription,
      authors: [author(101, 'Artist Alpha', 'public', true, null, avatarUrl)]
    });
  });
  await page.route(missingAvatar, async (route) => {
    await route.fulfill({ status: 404 });
  });
  await page.route(recoveredAvatar, async (route) => {
    await route.fulfill({
      status: 200,
      contentType: 'image/svg+xml',
      body: '<svg xmlns="http://www.w3.org/2000/svg" width="42" height="42"><rect width="42" height="42" fill="#0096fa"/></svg>'
    });
  });

  await page.goto('/discovery/following');
  const row = page.locator('.author-row').filter({ hasText: 'Artist Alpha' });
  await expect(row.locator('.fallback-text')).toHaveText('A');
  await expect(row.getByRole('img', { name: 'Artist Alpha' })).toHaveCount(0);

  await page.getByRole('button', { name: '刷新关注列表' }).click();
  await expect(row.getByRole('img', { name: 'Artist Alpha' })).toHaveAttribute(
    'src',
    recoveredAvatar
  );
});

test('following keeps cached authors while a failed refresh is retried', async ({
  page
}) => {
  await mockApi(page);
  const subscription = fixedSubscription(true, 3);
  let loadsUnavailable = false;

  await page.route(/\/api\/following$/, async (route) => {
    if (loadsUnavailable) {
      await fulfillJson(route, 503, { error: 'unavailable' });
      return;
    }
    await fulfillJson(route, 200, {
      subscription,
      authors: [
        author(101, 'Artist Alpha', 'public', true, '2026-07-31T12:30:00Z')
      ]
    });
  });

  await page.goto('/discovery/following');
  await expect(page.getByText('Artist Alpha')).toBeVisible();
  await expect(
    page.getByRole('button', { name: '重新读取关注列表' })
  ).toHaveCount(0);

  loadsUnavailable = true;
  await page.evaluate(() =>
    document.dispatchEvent(new Event('visibilitychange'))
  );
  await expect(page.getByText('关注列表暂时无法读取')).toBeVisible();
  await expect(page.getByText('Artist Alpha')).toBeVisible();
  const retryMessage = page
    .getByRole('alert')
    .filter({ hasText: '关注列表暂时无法读取' });
  await expect(retryMessage).toHaveCSS('border-top-style', 'solid');
  await expect(retryMessage).toHaveCSS('border-top-width', '1px');
  await expect(retryMessage).not.toHaveCSS(
    'background-color',
    'rgba(0, 0, 0, 0)'
  );

  loadsUnavailable = false;
  await page.getByRole('button', { name: '重新读取关注列表' }).click();
  await expect(page.getByText('关注列表暂时无法读取')).toHaveCount(0);
  await expect(
    page.getByRole('button', { name: '重新读取关注列表' })
  ).toHaveCount(0);
  await expect(page.getByText('Artist Alpha')).toBeVisible();
});

interface FollowingMockState {
  subscriptionUpdates: Array<Record<string, unknown>>;
  authorUpdates: Array<{
    pixivArtistId: number;
    expectedAccountId: string;
    enabled: boolean;
  }>;
  authorBatchUpdates: Array<{
    expectedAccountId: string;
    pixivArtistIds: number[];
    enabled: boolean;
  }>;
  refreshRequests: string[];
  runRequests: Array<{ expectedAccountId: string; backfill: boolean }>;
}

async function mockFollowing(page: Page): Promise<FollowingMockState> {
  await mockApi(page);
  const state: FollowingMockState = {
    subscriptionUpdates: [],
    authorUpdates: [],
    authorBatchUpdates: [],
    refreshRequests: [],
    runRequests: []
  };
  let subscription = fixedSubscription(true, 3);
  let subscriptionRevision = 3;
  const lastFullReconciledAt = '2026-07-31T09:53:00Z';
  let authors = [
    author(101, 'Artist Alpha', 'public', true, '2026-07-31T12:30:00Z'),
    author(102, 'Artist Beta', 'private', true, null)
  ];

  await page.route('**/api/following', async (route) => {
    if (route.request().method() === 'GET') {
      await fulfillJson(route, 200, {
        subscription,
        authors,
        last_full_reconciled_at: lastFullReconciledAt
      });
      return;
    }
    const request = route.request().postDataJSON() as Record<string, unknown>;
    state.subscriptionUpdates.push(request);
    subscriptionRevision += 1;
    subscription = fixedSubscription(
      Boolean(request.enabled),
      subscriptionRevision,
      Number(request.interval_minutes)
    );
    await fulfillJson(route, 200, subscription);
  });

  await page.route(/\/api\/following\/authors\/(\d+)$/, async (route) => {
    const pixivArtistId = Number(route.request().url().split('/').at(-1));
    const request = route.request().postDataJSON() as {
      expected_account_id: string;
      enabled: boolean;
    };
    state.authorUpdates.push({
      pixivArtistId,
      expectedAccountId: request.expected_account_id,
      enabled: request.enabled
    });
    authors = authors.map((item) =>
      item.pixiv_artist_id === pixivArtistId
        ? { ...item, enabled: request.enabled }
        : item
    );
    await fulfillJson(
      route,
      200,
      authors.find((item) => item.pixiv_artist_id === pixivArtistId)
    );
  });

  await page.route('**/api/following/authors', async (route) => {
    const request = route.request().postDataJSON() as {
      expected_account_id: string;
      pixiv_artist_ids: number[];
      enabled: boolean;
    };
    state.authorBatchUpdates.push({
      expectedAccountId: request.expected_account_id,
      pixivArtistIds: request.pixiv_artist_ids,
      enabled: request.enabled
    });
    const selected = new Set(request.pixiv_artist_ids);
    authors = authors.map((item) =>
      selected.has(item.pixiv_artist_id)
        ? { ...item, enabled: request.enabled }
        : item
    );
    await fulfillJson(route, 200, {
      subscription,
      authors,
      last_full_reconciled_at: lastFullReconciledAt
    });
  });

  await page.route('**/api/following/refresh', async (route) => {
    const request = route.request().postDataJSON() as {
      expected_account_id: string;
    };
    state.refreshRequests.push(request.expected_account_id);
    authors = [...authors, author(103, 'Artist Gamma', 'public', true, null)];
    await fulfillJson(route, 200, {
      subscription,
      authors,
      last_full_reconciled_at: lastFullReconciledAt
    });
  });

  await page.route('**/api/following/run', async (route) => {
    const request = route.request().postDataJSON() as {
      expected_account_id: string;
      backfill: boolean;
    };
    state.runRequests.push({
      expectedAccountId: request.expected_account_id,
      backfill: request.backfill
    });
    await fulfillJson(route, 202, {
      subscription_id: subscriptionId,
      run_id: '0198f652-0000-7000-8000-000000000011',
      job_id: '0198f652-0000-7000-8000-000000000012',
      trigger_kind: 'manual'
    });
  });

  return state;
}

function fixedSubscription(
  enabled: boolean,
  revision: number,
  intervalMinutes = 15
) {
  return {
    id: subscriptionId,
    account_id: PIXIV_ACCOUNT_ID,
    account_state: 'normal',
    rule_id: null,
    name: 'Pixiv关注动态',
    kind: 'following',
    enabled,
    schedule: { interval_minutes: intervalMinutes, lookback_pages: 2 },
    params: { source: 'following', mode: 'all' },
    next_run_at: '2026-08-01T00:00:00Z',
    pending_run: false,
    recent_state: 'succeeded',
    revision
  };
}

function author(
  pixivArtistId: number,
  displayName: string,
  visibility: 'public' | 'private',
  enabled: boolean,
  lastCollectedAt: string | null,
  avatarUrl: string | null = null
) {
  return {
    pixiv_artist_id: pixivArtistId,
    display_name: displayName,
    avatar_url: avatarUrl,
    visibility,
    enabled,
    refreshed_at: '2026-07-31T12:00:00Z',
    last_collected_at: lastCollectedAt
  };
}
