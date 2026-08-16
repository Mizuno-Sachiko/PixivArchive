import { expect, test } from '@playwright/test';

import { mockApi } from './support';

const WORK_ID = '0198f64c-42a2-7374-bace-9f1c3b317fc1';
const RESTORABLE_CAPABILITIES = {
  can_restore: true,
  can_reschedule: true,
  blocked_reason: null
} as const;

test('trash waits for its first snapshot without showing false empty data', async ({
  page
}) => {
  await mockApi(page);
  let releaseResponses: (() => void) | undefined;
  const responsesReleased = new Promise<void>((resolve) => {
    releaseResponses = resolve;
  });
  let deletionMarkerRequests = 0;
  page.on('request', (request) => {
    if (new URL(request.url()).pathname.startsWith('/api/deletion-markers')) {
      deletionMarkerRequests += 1;
    }
  });

  await page.route('**/api/trash?*', async (route) => {
    await responsesReleased;
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
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
      })
    });
  });
  await page.goto('/system/trash');
  await expect(
    page.locator('.trash-metrics [aria-label="正在读取"]')
  ).toHaveCount(3);
  await expect(page.getByText('回收站是空的')).toHaveCount(0);
  await expect(page.getByText('正在读取回收站数据…')).toBeVisible();

  releaseResponses?.();
  await expect(page.getByText('回收站是空的')).toBeVisible();
  const totalCount = page.getByText('0件', { exact: true });
  await expect(totalCount).toBeVisible();
  await expect(totalCount).toHaveClass(/panel-count/);
  await expect(page.getByText(/已选择/)).toHaveCount(0);
  expect(deletionMarkerRequests).toBe(0);
  await expect(
    page.locator('.trash-metrics [aria-label="正在读取"]')
  ).toHaveCount(0);
});

test('trash restores, reschedules, and purges without exposing deletion markers', async ({
  page
}) => {
  await mockApi(page);
  const commands: Array<{ method: string; url: string; body: unknown }> = [];
  await page.route('**/api/trash?*', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        items: [
          {
            work_id: WORK_ID,
            pixiv_work_id: 9911,
            title: '待清理作品',
            artist_name: 'Archive',
            page_count: 2,
            previous_collection_state: 'collected',
            trashed_at: '2026-07-20T12:00:00Z',
            scheduled_purge_at: '2026-08-19T12:00:00Z',
            purge_state: 'pending',
            purge_attempts: 0,
            failure_message: null,
            capabilities: RESTORABLE_CAPABILITIES,
            estimated_release_bytes: 6291456
          },
          {
            work_id: '0198f64c-42a2-7374-bace-9f1c3b317fc3',
            pixiv_work_id: 9912,
            title: '第二件待清理作品',
            artist_name: 'Archive',
            page_count: 1,
            previous_collection_state: 'collected',
            trashed_at: '2026-07-21T12:00:00Z',
            scheduled_purge_at: '2026-08-20T12:00:00Z',
            purge_state: 'pending',
            purge_attempts: 0,
            failure_message: null,
            capabilities: RESTORABLE_CAPABILITIES,
            estimated_release_bytes: 4194304
          }
        ],
        next_cursor: null,
        summary: {
          total_count: 2,
          logical_bytes: 10485760,
          estimated_reclaimable_bytes: 8388608
        },
        all_summary: {
          total_count: 2,
          logical_bytes: 10485760,
          estimated_reclaimable_bytes: 8388608
        }
      })
    })
  );
  await page.route('**/api/trash/**', async (route) => {
    const postData = route.request().postData();
    commands.push({
      method: route.request().method(),
      url: route.request().url(),
      body: postData ? route.request().postDataJSON() : null
    });
    await route.fulfill({
      status: route.request().url().endsWith('/purge') ? 202 : 204,
      contentType: 'application/json',
      body: route.request().url().endsWith('/purge')
        ? JSON.stringify({ job_id: '0198f64c-42a2-7374-bace-9f1c3b317fc2' })
        : ''
    });
  });
  await page.goto('/system/trash');
  await expect(page.getByRole('heading', { name: '回收站' })).toBeVisible();
  await expect(page.getByText('10.0 MiB')).toBeVisible();
  await expect(page.getByText('8.0 MiB')).toBeVisible();
  await expect(page.getByText('0件已选择', { exact: true })).toHaveCount(0);
  await expect(page.getByText('轻量删除标记')).toHaveCount(0);
  await expect(page.locator('article input[type="checkbox"]')).toHaveCount(0);
  await expect(page.getByRole('button', { name: '多选' })).toBeVisible();
  const heading = page.locator('.panel-heading').filter({
    has: page.getByRole('heading', { name: '待清理作品' })
  });
  await expect(heading.getByLabel('作品筛选')).toBeVisible();
  await expect(heading.getByRole('button', { name: '清理状态' })).toBeVisible();

  const firstItem = page
    .getByRole('article')
    .filter({ has: page.getByText('待清理作品', { exact: true }) });
  await expect(firstItem.locator('.status-pill')).toHaveCount(0);
  const detailLink = firstItem.getByRole('link', {
    name: '查看待清理作品详情'
  });
  await expect(detailLink).toHaveAttribute('href', '/gallery/works/9911');
  await expect(
    firstItem.getByRole('group', { name: '计划清理时间' })
  ).toBeVisible();
  const actionButtons = firstItem.locator('.row-actions button');
  const [detailBox, scheduleBox, restoreBox, actionBoxes] = await Promise.all([
    detailLink.boundingBox(),
    firstItem.getByRole('group', { name: '计划清理时间' }).boundingBox(),
    firstItem.getByRole('button', { name: '恢复' }).boundingBox(),
    actionButtons.evaluateAll((buttons) =>
      buttons.map((button) => button.getBoundingClientRect())
    )
  ]);
  expect(detailBox).not.toBeNull();
  expect(scheduleBox).not.toBeNull();
  expect(restoreBox).not.toBeNull();
  expect(Math.abs(detailBox!.height - scheduleBox!.height)).toBeLessThanOrEqual(
    1
  );
  expect(
    Math.abs(restoreBox!.height - scheduleBox!.height)
  ).toBeLessThanOrEqual(1);
  const controlBottoms = [
    detailBox!.y + detailBox!.height,
    scheduleBox!.y + scheduleBox!.height,
    ...actionBoxes.map((box) => box.bottom)
  ];
  expect(
    Math.max(...controlBottoms) - Math.min(...controlBottoms)
  ).toBeLessThanOrEqual(1);
  const scheduleLabel = firstItem.getByText('计划清理时间', { exact: true });
  expect(
    await scheduleLabel
      .locator('xpath=ancestor::*[1]')
      .getByRole('link')
      .count()
  ).toBe(0);

  await page.getByRole('button', { name: '恢复' }).first().click();
  await expect(page.getByText('作品已恢复')).toBeVisible();

  await expect(page.locator('input[type="datetime-local"]')).toHaveCount(0);
  await firstItem.getByRole('button', { name: '选择计划清理时间' }).click();
  await expect(page.getByRole('grid')).toBeVisible();
  await page.keyboard.press('Escape');
  await firstItem.getByRole('button', { name: '保存日期' }).click();

  await firstItem.getByRole('button', { name: '立即清理' }).click();
  const confirmation = page.getByRole('dialog', { name: '立即清理作品' });
  await expect(confirmation).toContainText('待清理作品');
  await confirmation.getByRole('button', { name: '立即清理' }).click();

  await expect
    .poll(() => commands.map(({ method }) => method))
    .toEqual(['POST', 'PUT', 'POST']);
});

test('trash sends selected work as one server-side batch', async ({ page }) => {
  await mockApi(page);
  const failedWorkId = '0198f64c-42a2-7374-bace-9f1c3b317fc3';
  const items = [
    {
      work_id: WORK_ID,
      pixiv_work_id: 9911,
      title: '待清理作品',
      artist_name: 'Archive',
      page_count: 2,
      previous_collection_state: 'collected',
      trashed_at: '2026-07-20T12:00:00Z',
      scheduled_purge_at: '2026-08-19T12:00:00Z',
      purge_state: 'pending',
      purge_attempts: 0,
      failure_message: null,
      capabilities: RESTORABLE_CAPABILITIES,
      estimated_release_bytes: 6291456
    },
    {
      work_id: failedWorkId,
      pixiv_work_id: 9912,
      title: '第二件待清理作品',
      artist_name: 'Archive',
      page_count: 1,
      previous_collection_state: 'collected',
      trashed_at: '2026-07-21T12:00:00Z',
      scheduled_purge_at: '2026-08-20T12:00:00Z',
      purge_state: 'pending',
      purge_attempts: 0,
      failure_message: null,
      capabilities: RESTORABLE_CAPABILITIES,
      estimated_release_bytes: 4194304
    }
  ];
  await page.route('**/api/trash?*', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        items,
        next_cursor: null,
        summary: {
          total_count: 2,
          logical_bytes: 10485760,
          estimated_reclaimable_bytes: 10485760
        },
        all_summary: {
          total_count: 2,
          logical_bytes: 10485760,
          estimated_reclaimable_bytes: 10485760
        }
      })
    })
  );
  await page.route('**/api/trash/selection', async (route) => {
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
        selected_count: baseSelected
          ? items.length - exceptions.size
          : exceptions.size,
        blocked_count: 0,
        selected_visible_work_ids: request.visible_work_ids.filter(
          (workId) => baseSelected !== exceptions.has(workId)
        )
      })
    });
  });
  const batches: unknown[] = [];
  await page.route('**/api/trash/restore', async (route) => {
    batches.push(route.request().postDataJSON());
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({ affected_count: 2 })
    });
  });

  await page.goto('/system/trash');
  await expect(page.getByText('0件已选择', { exact: true })).toHaveCount(0);
  await page.getByRole('button', { name: '多选' }).click();
  await expect(page.getByRole('button', { name: '退出多选' })).toBeVisible();
  await page.getByLabel('选择待清理作品').check();
  await page.getByLabel('选择第二件待清理作品').check();
  await expect(page.getByText('2件已选择')).toHaveClass(/panel-count/);
  const panelHeading = page.locator('.panel-heading');
  await expect(
    panelHeading.getByRole('button', { name: '批量恢复' })
  ).toBeVisible();
  const [selectedBox, totalBox] = await Promise.all([
    panelHeading.getByText('2件已选择').boundingBox(),
    panelHeading.getByText('2件', { exact: true }).boundingBox()
  ]);
  expect(selectedBox).not.toBeNull();
  expect(totalBox).not.toBeNull();
  expect(selectedBox!.x).toBeLessThan(totalBox!.x);
  await page.getByRole('button', { name: '批量恢复' }).click();

  await expect(page.getByText('所选作品已恢复')).toBeVisible();
  await expect(page.getByRole('button', { name: '多选' })).toBeVisible();
  await expect(page.locator('article input[type="checkbox"]')).toHaveCount(0);
  expect(batches).toEqual([
    {
      expression: {
        filter: { query: null, purge_states: [] },
        base_selected: false,
        exception_work_ids: [WORK_ID, failedWorkId]
      }
    }
  ]);
});

test('trash fixes the applied filter while selection replaces its controls', async ({
  page
}) => {
  await mockApi(page);
  const firstWorkId = WORK_ID;
  const selectionRequests: unknown[] = [];
  const restoreRequests: unknown[] = [];

  await page.route('**/api/trash?*', async (route) => {
    const query = new URL(route.request().url()).searchParams.get('query');
    const item = trashItem(firstWorkId, query ? `${query}作品` : '初始作品');
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        items: [item],
        next_cursor: null,
        summary: {
          total_count: 1,
          logical_bytes: 1048576,
          estimated_reclaimable_bytes: 1048576
        },
        all_summary: {
          total_count: 1,
          logical_bytes: 1048576,
          estimated_reclaimable_bytes: 1048576
        }
      })
    });
  });
  await page.route('**/api/trash/selection', async (route) => {
    const body = route.request().postDataJSON() as {
      expression: { base_selected?: boolean; exception_work_ids?: string[] };
      visible_work_ids: string[];
    };
    selectionRequests.push(body);
    const exceptions = new Set(body.expression.exception_work_ids ?? []);
    const baseSelected = body.expression.base_selected ?? false;
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        selected_count: baseSelected ? 1 - exceptions.size : exceptions.size,
        blocked_count: 0,
        selected_visible_work_ids: body.visible_work_ids.filter(
          (workId) => baseSelected !== exceptions.has(workId)
        )
      })
    });
  });
  await page.route('**/api/trash/restore', async (route) => {
    restoreRequests.push(route.request().postDataJSON());
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({ affected_count: 1 })
    });
  });

  await page.goto('/system/trash');
  await page.getByLabel('作品筛选').fill('第一组');
  await page.getByRole('button', { name: '筛选' }).click();
  await expect(page.getByText('第一组作品', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: '多选' }).click();
  await expect(page.getByLabel('作品筛选')).toHaveCount(0);
  await expect(page.getByRole('button', { name: '筛选' })).toHaveCount(0);
  await page.getByRole('button', { name: '全选' }).click();
  await expect(page.getByText('1件已选择')).toBeVisible();
  await expect(page.getByLabel('选择第一组作品')).toBeChecked();

  await page.getByRole('button', { name: '批量恢复' }).click();
  await expect(page.getByLabel('作品筛选')).toHaveValue('第一组');

  expect(selectionRequests).toEqual([
    {
      expression: {
        filter: { query: '第一组', purge_states: [] },
        base_selected: true,
        exception_work_ids: []
      },
      visible_work_ids: [firstWorkId]
    }
  ]);
  expect(restoreRequests).toEqual([
    {
      expression: {
        filter: { query: '第一组', purge_states: [] },
        base_selected: true,
        exception_work_ids: []
      }
    }
  ]);
});

test('trash range selection supports invert and clear without changing a rejected selection', async ({
  page
}) => {
  await mockApi(page);
  const firstWorkId = WORK_ID;
  const secondWorkId = '0198f64c-42a2-7374-bace-9f1c3b317fc3';
  const items = [
    trashItem(firstWorkId, '第一件作品'),
    trashItem(secondWorkId, '第二件作品')
  ];
  let projectionCount = 0;
  await page.route('**/api/trash?*', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        items,
        next_cursor: null,
        summary: {
          total_count: 2,
          logical_bytes: 2097152,
          estimated_reclaimable_bytes: 2097152
        },
        all_summary: {
          total_count: 2,
          logical_bytes: 2097152,
          estimated_reclaimable_bytes: 2097152
        }
      })
    })
  );
  await page.route('**/api/trash/selection', async (route) => {
    projectionCount += 1;
    if (projectionCount === 2) {
      await route.fulfill({
        status: 503,
        contentType: 'application/json',
        body: JSON.stringify({
          code: 'service_unavailable',
          message: 'Selection projection is temporarily unavailable',
          details: {},
          trace_id: '0198f64d-477c-7d1e-aa2d-c74bb29ea4d7'
        })
      });
      return;
    }
    const request = route.request().postDataJSON() as {
      visible_work_ids: string[];
    };
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        selected_count: 1,
        blocked_count: 0,
        selected_visible_work_ids: request.visible_work_ids.filter(
          (workId) => workId === firstWorkId
        )
      })
    });
  });

  await page.goto('/system/trash');
  await page.getByRole('button', { name: '多选' }).click();
  await page.getByLabel('选择第一件作品').check();
  await expect(page.getByText('1件已选择')).toBeVisible();
  await expect(page.getByLabel('选择第一件作品')).toBeChecked();

  await page.getByRole('button', { name: '全选' }).click();
  await expect(page.getByText('无法更新当前选择')).toBeVisible();
  await expect(page.getByText('1件已选择')).toBeVisible();
  await expect(page.getByLabel('选择第一件作品')).toBeChecked();
  await expect(page.getByLabel('选择第二件作品')).not.toBeChecked();

  await page.getByRole('button', { name: '全不选' }).click();
  await expect(page.getByText(/件已选择/)).toHaveCount(0);
  await expect(page.getByLabel('选择第一件作品')).not.toBeChecked();
});

test('trash selects more than 500 matching works without loading every id', async ({
  page
}) => {
  await mockApi(page);
  const items = [trashItem(WORK_ID, '待清理作品1')];
  await page.route('**/api/trash?*', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        items,
        next_cursor: null,
        summary: {
          total_count: 501,
          logical_bytes: 501 * 1048576,
          estimated_reclaimable_bytes: 501 * 1048576
        },
        all_summary: {
          total_count: 501,
          logical_bytes: 501 * 1048576,
          estimated_reclaimable_bytes: 501 * 1048576
        }
      })
    })
  );
  await page.route('**/api/trash/selection', async (route) => {
    const request = route.request().postDataJSON() as {
      visible_work_ids: string[];
    };
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        selected_count: 501,
        blocked_count: 0,
        selected_visible_work_ids: request.visible_work_ids
      })
    });
  });
  let restoreRequest: unknown;
  await page.route('**/api/trash/restore', async (route) => {
    restoreRequest = route.request().postDataJSON();
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({ affected_count: 501 })
    });
  });

  await page.goto('/system/trash');
  await page.getByRole('button', { name: '多选' }).click();
  await page.getByRole('button', { name: '全选' }).click();
  await expect(page.getByText('501件已选择')).toBeVisible();
  await expect(page.getByLabel('选择待清理作品1')).toBeChecked();
  await page.getByRole('button', { name: '批量恢复' }).click();
  expect(restoreRequest).toEqual({
    expression: {
      filter: { query: null, purge_states: [] },
      base_selected: true,
      exception_work_ids: []
    }
  });
});

test('trash keeps selection and shows the server batch error', async ({
  page
}) => {
  await mockApi(page);
  const secondWorkId = '0198f64c-42a2-7374-bace-9f1c3b317fc3';
  const items = [
    trashItem(WORK_ID, '待清理作品'),
    trashItem(secondWorkId, '第二件待清理作品')
  ];
  await page.route('**/api/trash?*', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        items,
        next_cursor: null,
        summary: {
          total_count: 2,
          logical_bytes: 2097152,
          estimated_reclaimable_bytes: 2097152
        },
        all_summary: {
          total_count: 2,
          logical_bytes: 2097152,
          estimated_reclaimable_bytes: 2097152
        }
      })
    })
  );
  await page.route('**/api/trash/restore', (route) =>
    route.fulfill({
      status: 409,
      contentType: 'application/json',
      body: JSON.stringify({
        code: 'conflict',
        message: '回收站状态已变化，请重试',
        details: {},
        trace_id: '0198f64d-477c-7d1e-aa2d-c74bb29ea4d7'
      })
    })
  );
  await page.route('**/api/trash/selection', async (route) => {
    const request = route.request().postDataJSON() as {
      expression: { exception_work_ids?: string[] };
      visible_work_ids: string[];
    };
    const selected = new Set(request.expression.exception_work_ids ?? []);
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        selected_count: selected.size,
        blocked_count: 0,
        selected_visible_work_ids: request.visible_work_ids.filter((workId) =>
          selected.has(workId)
        )
      })
    });
  });

  await page.goto('/system/trash');
  await page.getByRole('button', { name: '多选' }).click();
  await page.getByLabel('选择待清理作品').check();
  await page.getByLabel('选择第二件待清理作品').check();
  await page.getByRole('button', { name: '批量恢复' }).click();

  await expect(page.getByText('回收站状态已变化，请重试')).toBeVisible();
  await expect(page.getByText('2件已选择')).toBeVisible();
  await expect(page.getByRole('button', { name: '退出多选' })).toBeVisible();
  await expect(page.getByLabel('选择待清理作品')).toBeChecked();
});

test('trash pauses event refresh during multi-select and catches up after exit', async ({
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
      body: `retry: 60000\nevent: app_event\ndata: {"resource":"work","resource_id":"${WORK_ID}"}\n\n`
    });
  });
  let refreshed = false;
  let listRequests = 0;
  await page.route('**/api/trash?*', async (route) => {
    listRequests += 1;
    const item = refreshed
      ? trashItem(WORK_ID, '事件刷新后的作品')
      : trashItem(WORK_ID, '待清理作品');
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        items: [item],
        next_cursor: null,
        summary: {
          total_count: 1,
          logical_bytes: 1048576,
          estimated_reclaimable_bytes: 1048576
        },
        all_summary: {
          total_count: 1,
          logical_bytes: 1048576,
          estimated_reclaimable_bytes: 1048576
        }
      })
    });
  });
  await page.route('**/api/trash/selection', async (route) => {
    const request = route.request().postDataJSON() as {
      expression: { exception_work_ids?: string[] };
      visible_work_ids: string[];
    };
    const selected = new Set(request.expression.exception_work_ids ?? []);
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        selected_count: selected.size,
        blocked_count: 0,
        selected_visible_work_ids: request.visible_work_ids.filter((workId) =>
          selected.has(workId)
        )
      })
    });
  });

  await page.goto('/system/trash');
  await page.getByRole('button', { name: '多选' }).click();
  await page.getByLabel('选择待清理作品').check();
  refreshed = true;
  const eventResponse = page.waitForResponse('**/api/events');
  publishWorkChange();

  await (await eventResponse).finished();
  await expect(
    page.locator('article.trash-work').getByText('待清理作品', { exact: true })
  ).toBeVisible();
  await expect(page.getByLabel('选择待清理作品')).toBeChecked();
  expect(listRequests).toBe(1);

  await page.getByRole('button', { name: '退出多选' }).click();
  await expect(page.getByText('事件刷新后的作品')).toBeVisible();
  await expect(page.locator('article input[type="checkbox"]')).toHaveCount(0);
  expect(listRequests).toBe(2);
});

test('trash presents cleanup states in user-facing language', async ({
  page
}) => {
  await mockApi(page);
  await page.route('**/api/trash?*', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        items: [
          trashItem(WORK_ID, '等待作品', 'pending'),
          trashItem(
            '0198f64c-42a2-7374-bace-9f1c3b317fc3',
            '清理中作品',
            'running'
          ),
          {
            ...trashItem(
              '0198f64c-42a2-7374-bace-9f1c3b317fc4',
              '失败作品',
              'failed'
            ),
            failure_message: '媒体文件暂时无法删除'
          }
        ],
        next_cursor: null,
        summary: {
          total_count: 3,
          logical_bytes: 3145728,
          estimated_reclaimable_bytes: 3145728
        },
        all_summary: {
          total_count: 3,
          logical_bytes: 3145728,
          estimated_reclaimable_bytes: 3145728
        }
      })
    })
  );

  await page.goto('/system/trash');

  await expect(page.getByText('状态 pending')).toHaveCount(0);
  await expect(page.getByText('状态 running')).toHaveCount(0);
  await expect(page.getByText('状态 failed')).toHaveCount(0);
  await expect(page.getByText('正在清理', { exact: true })).toBeVisible();
  await expect(page.getByText('清理失败', { exact: true })).toBeVisible();
  await expect(page.getByText('媒体文件暂时无法删除')).toBeVisible();
});

test('trash pages and filters on the server while clear-all keeps collection scope', async ({
  page
}) => {
  await mockApi(page);
  const cursorWorkId = '0198f64c-42a2-7374-bace-9f1c3b317fc4';
  const requests: string[] = [];
  let purgeAllRequests = 0;

  await page.route('**/api/trash?*', async (route) => {
    const url = new URL(route.request().url());
    requests.push(url.search);
    const filtered = url.searchParams.get('query') === '失败作品';
    const continued = url.searchParams.has('cursor_work_id');
    const item = filtered
      ? trashItem('0198f64c-42a2-7374-bace-9f1c3b317fc5', '失败作品', 'failed')
      : continued
        ? trashItem(cursorWorkId, '继续加载的作品')
        : trashItem(WORK_ID, '第一页作品');
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        items: [item],
        next_cursor:
          filtered || continued
            ? null
            : {
                scheduled_purge_at: item.scheduled_purge_at,
                work_id: item.work_id
              },
        summary: filtered
          ? {
              total_count: 1,
              logical_bytes: 1048576,
              estimated_reclaimable_bytes: 1048576
            }
          : {
              total_count: 205,
              logical_bytes: 10485760,
              estimated_reclaimable_bytes: 8388608
            },
        all_summary: {
          total_count: 205,
          logical_bytes: 10485760,
          estimated_reclaimable_bytes: 8388608
        }
      })
    });
  });
  await page.route('**/api/trash/purge-all', async (route) => {
    purgeAllRequests += 1;
    await route.fulfill({
      status: 202,
      contentType: 'application/json',
      body: JSON.stringify({ accepted_count: 205 })
    });
  });

  await page.goto('/system/trash');
  await expect(
    page.getByText('205件，已显示1件', { exact: true })
  ).toBeVisible();
  await page.getByRole('button', { name: '继续加载' }).click();
  await expect(page.getByText('继续加载的作品')).toBeVisible();
  await expect(
    page.getByText('205件，已显示2件', { exact: true })
  ).toBeVisible();

  await page.getByLabel('作品筛选').fill('失败作品');
  const purgeState = page.getByRole('button', { name: '清理状态' });
  await purgeState.click();
  await page.getByRole('option', { name: '等待清理' }).click();
  await expect(purgeState).toContainText('等待清理');
  await page.getByRole('button', { name: '筛选' }).click();
  await expect(page.getByText('失败作品', { exact: true })).toBeVisible();
  await expect(page.getByText('1件', { exact: true })).toBeVisible();
  expect(requests.some((search) => search.includes('cursor_work_id='))).toBe(
    true
  );
  expect(
    requests.some((search) => {
      const params = new URLSearchParams(search);
      return (
        params.get('query') === '失败作品' &&
        params.get('purge_state') === 'pending' &&
        params.getAll('purge_state').length === 1
      );
    })
  ).toBe(true);

  await page.getByRole('button', { name: '清空全部' }).click();
  const confirmation = page.getByRole('dialog', { name: '清空回收站' });
  await expect(confirmation).toContainText('205件');
  await expect(confirmation).toContainText('8.0 MiB');
  await confirmation.getByRole('button', { name: '立即清理' }).click();
  await expect.poll(() => purgeAllRequests).toBe(1);
});

test('trash discards an older pagination response after filters change', async ({
  page
}) => {
  await mockApi(page);
  let releaseOlderPage: (() => void) | undefined;
  const olderPageReleased = new Promise<void>((resolve) => {
    releaseOlderPage = resolve;
  });

  await page.route('**/api/trash?*', async (route) => {
    const url = new URL(route.request().url());
    const filtered = url.searchParams.get('query') === '新筛选';
    const continued = url.searchParams.has('cursor_work_id');
    if (continued) await olderPageReleased;
    const item = filtered
      ? trashItem('0198f64c-42a2-7374-bace-9f1c3b317fc6', '新筛选作品')
      : continued
        ? trashItem('0198f64c-42a2-7374-bace-9f1c3b317fc7', '旧分页作品')
        : trashItem(WORK_ID, '初始作品');
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        items: [item],
        next_cursor:
          filtered || continued
            ? null
            : {
                scheduled_purge_at: item.scheduled_purge_at,
                work_id: item.work_id
              },
        summary: {
          total_count: 1,
          logical_bytes: 1048576,
          estimated_reclaimable_bytes: 1048576
        },
        all_summary: {
          total_count: 2,
          logical_bytes: 2097152,
          estimated_reclaimable_bytes: 2097152
        }
      })
    });
  });

  await page.goto('/system/trash');
  await page.getByRole('button', { name: '继续加载' }).click();
  await page.getByLabel('作品筛选').fill('新筛选');
  await page.getByRole('button', { name: '筛选' }).click();
  await expect(page.getByText('新筛选作品')).toBeVisible();
  releaseOlderPage?.();

  await expect(page.getByText('旧分页作品')).toHaveCount(0);
});

test('trash keeps filter controls stable while a replacement snapshot is loading', async ({
  page
}) => {
  await mockApi(page);
  let reportRefreshStarted!: () => void;
  const refreshStarted = new Promise<void>((resolve) => {
    reportRefreshStarted = resolve;
  });
  let releaseRefresh!: () => void;
  const refreshReleased = new Promise<void>((resolve) => {
    releaseRefresh = resolve;
  });

  await page.route('**/api/trash?*', async (route) => {
    const url = new URL(route.request().url());
    const filtered = url.searchParams.get('query') === '新筛选';
    if (filtered) {
      reportRefreshStarted();
      await refreshReleased;
    }
    const item = filtered
      ? trashItem('0198f64c-42a2-7374-bace-9f1c3b317fc8', '新筛选作品')
      : trashItem(WORK_ID, '初始作品');
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        items: [item],
        next_cursor: filtered
          ? null
          : {
              scheduled_purge_at: item.scheduled_purge_at,
              work_id: item.work_id
            },
        summary: {
          total_count: 1,
          logical_bytes: 1048576,
          estimated_reclaimable_bytes: 1048576
        },
        all_summary: {
          total_count: 1,
          logical_bytes: 1048576,
          estimated_reclaimable_bytes: 1048576
        }
      })
    });
  });

  await page.goto('/system/trash');
  await expect(page.getByText('初始作品')).toBeVisible();
  const search = page.getByLabel('作品筛选');
  const purgeState = page.getByRole('button', { name: '清理状态' });
  const filterButton = page.getByRole('button', { name: '筛选' });
  const initialOpacity = await filterButton.evaluate(
    (element) => getComputedStyle(element).opacity
  );
  await search.fill('新筛选');
  await filterButton.click();
  await refreshStarted;

  const loadMore = page.getByRole('button', { name: '继续加载' });
  try {
    await expect(loadMore).toBeDisabled();
    await expect(search).toBeEnabled();
    await expect(purgeState).toBeEnabled();
    await expect(filterButton).toBeEnabled();
    await expect(filterButton).toHaveText('筛选');
    expect(
      await filterButton.evaluate(
        (element) => getComputedStyle(element).opacity
      )
    ).toBe(initialOpacity);
  } finally {
    releaseRefresh();
  }
  await expect(page.getByText('新筛选作品')).toBeVisible();
  await expect(filterButton).toBeEnabled();
});

test('trash restores its loaded depth, filter draft, and scroll position after opening a work', async ({
  page
}) => {
  await page.setViewportSize({ width: 1000, height: 600 });
  await mockApi(page);
  const items = Array.from({ length: 100 }, (_, index) => ({
    ...trashItem(batchWorkId(index + 1000), `分页作品${index + 1}`),
    pixiv_work_id: 20_000 + index
  }));
  const requests: string[] = [];
  await page.route('**/api/trash?*', async (route) => {
    const url = new URL(route.request().url());
    requests.push(url.search);
    const continued = url.searchParams.has('cursor_work_id');
    const pageItems = continued ? items.slice(50) : items.slice(0, 50);
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        items: pageItems,
        next_cursor: continued
          ? null
          : {
              scheduled_purge_at: pageItems.at(-1)!.scheduled_purge_at,
              work_id: pageItems.at(-1)!.work_id
            },
        summary: {
          total_count: 100,
          logical_bytes: 104_857_600,
          estimated_reclaimable_bytes: 83_886_080
        },
        all_summary: {
          total_count: 100,
          logical_bytes: 104_857_600,
          estimated_reclaimable_bytes: 83_886_080
        }
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

  await page.goto('/system/trash');
  await page.getByRole('button', { name: '继续加载' }).click();
  await expect(page.locator('article.trash-work')).toHaveCount(100);
  await page.getByLabel('作品筛选').fill('尚未应用的草稿');
  const anchor = page
    .getByRole('article')
    .filter({ has: page.getByText('分页作品81', { exact: true }) });
  await anchor.scrollIntoViewIfNeeded();
  const scrollBeforeOpen = await page.evaluate(() => window.scrollY);
  const requestsBeforeOpen = requests.length;
  await anchor.getByRole('link', { name: '查看分页作品81详情' }).click();
  await expect(page).toHaveURL('/gallery/works/20080');
  await page
    .getByRole('navigation', { name: '二级导航' })
    .getByRole('link', { name: '返回' })
    .click();

  await expect(page).toHaveURL('/system/trash');
  await expect(page.locator('article.trash-work')).toHaveCount(100);
  await expect(page.getByLabel('作品筛选')).toHaveValue('尚未应用的草稿');
  await expect(page.getByText('分页作品81', { exact: true })).toBeVisible();
  await expect
    .poll(() =>
      page.evaluate(
        (expected) => Math.abs(window.scrollY - expected),
        scrollBeforeOpen
      )
    )
    .toBeLessThan(4);
  expect(
    requests
      .slice(requestsBeforeOpen)
      .every((search) => !new URLSearchParams(search).has('query'))
  ).toBe(true);
});

test('trash keeps the prior applied filter when a replacement snapshot fails', async ({
  page
}) => {
  await mockApi(page);
  const selectionRequests: Array<{
    expression?: {
      filter?: { query?: string | null; purge_states?: string[] };
    };
  }> = [];
  await page.route('**/api/trash?*', async (route) => {
    const query = new URL(route.request().url()).searchParams.get('query');
    if (query === '失败筛选') {
      await route.fulfill({ status: 503, body: 'unavailable' });
      return;
    }
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        items: [trashItem(WORK_ID, '原筛选作品')],
        next_cursor: null,
        summary: {
          total_count: 1,
          logical_bytes: 1_048_576,
          estimated_reclaimable_bytes: 1_048_576
        },
        all_summary: {
          total_count: 1,
          logical_bytes: 1_048_576,
          estimated_reclaimable_bytes: 1_048_576
        }
      })
    });
  });
  await page.route('**/api/trash/selection', async (route) => {
    const request = route.request().postDataJSON() as {
      expression?: {
        filter?: { query?: string | null; purge_states?: string[] };
      };
      visible_work_ids: string[];
    };
    selectionRequests.push(request);
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        selected_count: 1,
        blocked_count: 0,
        selected_visible_work_ids: request.visible_work_ids
      })
    });
  });

  await page.goto('/system/trash');
  await page.getByLabel('作品筛选').fill('失败筛选');
  await page.getByRole('button', { name: '筛选' }).click();
  await expect(page.getByText('回收站数据暂时无法读取')).toBeVisible();
  await expect(page.getByText('原筛选作品', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: '多选' }).click();
  await page.getByRole('button', { name: '全选' }).click();

  expect(selectionRequests.at(-1)?.expression?.filter).toEqual({
    query: null,
    purge_states: []
  });
});

test('trash preserves an unsaved cleanup date during an event refresh', async ({
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
      body: `retry: 60000\nevent: app_event\ndata: {"resource":"work","resource_id":"${WORK_ID}"}\n\n`
    });
  });
  let listRequests = 0;
  await page.route('**/api/trash?*', async (route) => {
    listRequests += 1;
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        items: [trashItem(WORK_ID, '日期草稿作品')],
        next_cursor: null,
        summary: {
          total_count: 1,
          logical_bytes: 1_048_576,
          estimated_reclaimable_bytes: 1_048_576
        },
        all_summary: {
          total_count: 1,
          logical_bytes: 1_048_576,
          estimated_reclaimable_bytes: 1_048_576
        }
      })
    });
  });

  await page.goto('/system/trash');
  const row = page
    .getByRole('article')
    .filter({ has: page.getByText('日期草稿作品', { exact: true }) });
  const schedule = row.getByRole('group', { name: '计划清理时间' });
  const original = await schedule.textContent();
  await row.getByRole('button', { name: '选择计划清理时间' }).click();
  await page
    .locator(
      '.pa-calendar-day:not([data-selected]):not([data-outside-month]):not([data-disabled])'
    )
    .last()
    .click();
  await page.getByRole('button', { name: '完成日期时间选择' }).click();
  const draft = await schedule.textContent();
  expect(draft).not.toBe(original);

  publishWorkChange();
  await expect.poll(() => listRequests).toBeGreaterThan(1);
  await expect.poll(() => schedule.textContent()).toBe(draft);
});

test('trash removes a restored work even when the following refresh fails', async ({
  page
}) => {
  await mockApi(page);
  let listRequests = 0;
  await page.route('**/api/trash?*', async (route) => {
    listRequests += 1;
    if (listRequests > 1) {
      await route.fulfill({ status: 503, body: 'unavailable' });
      return;
    }
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        items: [trashItem(WORK_ID, '恢复后立即消失')],
        next_cursor: null,
        summary: {
          total_count: 1,
          logical_bytes: 8_388_608,
          estimated_reclaimable_bytes: 3_145_728
        },
        all_summary: {
          total_count: 1,
          logical_bytes: 8_388_608,
          estimated_reclaimable_bytes: 3_145_728
        }
      })
    });
  });
  await page.route(`**/api/trash/${WORK_ID}/restore`, (route) =>
    route.fulfill({ status: 204 })
  );

  await page.goto('/system/trash');
  await page.getByRole('button', { name: '恢复' }).click();

  await expect(page.getByText('恢复后立即消失', { exact: true })).toHaveCount(
    0
  );
  await expect(page.getByText('作品已恢复')).toBeVisible();
  await expect(page.getByText('回收站数据暂时无法读取')).toBeVisible();
  await expect(page.getByText('8.0 MiB')).toBeVisible();
  await expect(page.getByText('3.0 MiB')).toBeVisible();
});

function trashItem(workId: string, title: string, purgeState = 'pending') {
  return {
    work_id: workId,
    pixiv_work_id: 9911,
    title,
    artist_name: 'Archive',
    page_count: 1,
    previous_collection_state: 'collected',
    trashed_at: '2026-07-20T12:00:00Z',
    scheduled_purge_at: '2026-08-19T12:00:00Z',
    purge_state: purgeState,
    purge_attempts: 0,
    failure_message: null,
    capabilities: RESTORABLE_CAPABILITIES,
    estimated_release_bytes: 1048576
  };
}

function batchWorkId(index: number): string {
  return `0198f64c-42a2-7374-bace-${String(index).padStart(12, '0')}`;
}
