import { expect, test, type Page } from '@playwright/test';

import { fulfillJson, mockApi, type MockApiOptions } from './support';

const failedId = '0198f652-0000-7000-8000-000000000011';
const waitingId = '0198f652-0000-7000-8000-000000000012';

test('tasks filters errors and sends revision-safe retry and cancel commands', async ({
  page
}) => {
  const api = await mockTasks(page);
  await page.goto('/tasks/errors');

  await expect.poll(() => api.lastListLimit).toBe('200');

  await expect(
    page.getByRole('heading', { name: '任务与运行记录' })
  ).toBeVisible();
  await expect(
    page.getByRole('heading', { name: '任务队列', exact: true })
  ).toBeVisible();
  const recentCount = page.getByText('最近2项');
  await expect(recentCount).toHaveAttribute('title', '列表最多显示最近200项');
  await expect(recentCount).toHaveClass(/panel-count/);
  await expect(page.getByText('任务总数', { exact: true })).toHaveCount(0);
  for (const [label, value] of [
    ['运行中任务', '1'],
    ['正在等待', '2'],
    ['需要处理', '2']
  ] as const) {
    const metric = page.locator('.metric-strip > div').filter({
      hasText: label
    });
    await expect(metric.getByText(value, { exact: true })).toBeVisible();
  }
  const metricColumns = await page
    .locator('.metric-strip')
    .evaluate(
      (element) =>
        getComputedStyle(element).gridTemplateColumns.split(' ').length
    );
  expect(metricColumns).toBe(3);
  await expect(
    page.locator('.metric-strip > div').first().locator('strong')
  ).toHaveCSS('font-size', '26.4px');

  await expect(page.getByRole('tab')).toHaveCount(0);
  const failedRow = page.getByRole('button', { name: /下载原图/ });
  await failedRow.getByText('后台维护').click();
  const detail = page.getByRole('region', { name: '任务详情' });
  await expect(detail.getByText('网络错误', { exact: true })).toBeVisible();
  await expect(
    detail.getByText('0198f652-0000-7000-8000-000000000099')
  ).toBeVisible();
  await detail.getByRole('button', { name: '重试任务' }).click();
  await expect.poll(() => api.retryBody).toEqual({ expected_revision: 6 });

  await page.goto('/tasks');
  await expect(page.getByText('最近4项')).toBeVisible();
  await page.getByRole('button', { name: /排行榜采集/ }).press('Enter');
  await expect(detail.getByText('等待Pixiv账户')).toBeVisible();
  await detail.getByRole('button', { name: '取消任务' }).click();
  await expect.poll(() => api.cancelBody).toEqual({ expected_revision: 4 });

  await page
    .getByRole('button', {
      name: '生成浏览图 0198f652-0000-7000-8000-000000000014'
    })
    .click();
  await expect(detail.getByText('等待存储空间').first()).toBeVisible();
});

test('task snapshot is refreshed after an SSE invalidation', async ({
  page
}) => {
  const api = await mockTasks(page, { initialSnapshot: true });
  await page.goto('/tasks');

  await expect.poll(() => api.listRequests).toBeGreaterThan(1);
  await expect(page.getByText('正在处理')).toBeVisible();
});

interface TaskMockState {
  listRequests: number;
  lastListLimit?: string | null;
  retryBody?: Record<string, unknown>;
  cancelBody?: Record<string, unknown>;
}

async function mockTasks(
  page: Page,
  options: MockApiOptions = {}
): Promise<TaskMockState> {
  await mockApi(page, true, options);
  const state: TaskMockState = { listRequests: 0 };
  const tasks = [
    task(failedId, 'download_media', 'failed', 'network', 6),
    task(
      waitingId,
      'ranking_collection',
      'waiting_account',
      'credential_invalid',
      4
    ),
    task(
      '0198f652-0000-7000-8000-000000000013',
      'generate_derivative',
      'running',
      null,
      2
    ),
    task(
      '0198f652-0000-7000-8000-000000000014',
      'generate_derivative',
      'waiting_storage',
      null,
      3
    )
  ];

  await page.route('**/api/tasks?**', async (route) => {
    state.listRequests += 1;
    state.lastListLimit = new URL(route.request().url()).searchParams.get(
      'limit'
    );
    await fulfillJson(route, 200, {
      items: tasks,
      summary: {
        total: 1_234_567,
        running: 1,
        waiting: 2,
        requires_attention: 2
      }
    });
  });

  await page.route(`**/api/tasks/${failedId}`, async (route) => {
    await fulfillJson(route, 200, {
      task: tasks[0],
      attempts: [
        {
          attempt_number: 2,
          state: 'failed',
          started_at: '2026-07-30T02:00:00Z',
          finished_at: '2026-07-30T02:00:08Z',
          error_class: 'network',
          retryable: true,
          message: 'upstream connection reset',
          trace_id: '0198f652-0000-7000-8000-000000000099'
        }
      ]
    });
  });

  await page.route(`**/api/tasks/${waitingId}`, async (route) => {
    await fulfillJson(route, 200, {
      task: tasks[1],
      attempts: [
        {
          attempt_number: 1,
          state: 'failed',
          started_at: '2026-07-30T02:30:00Z',
          finished_at: '2026-07-30T02:30:01Z',
          error_class: 'credential_invalid',
          retryable: null,
          message: 'Pixiv authentication was rejected',
          trace_id: '0198f652-0000-7000-8000-000000000098'
        }
      ]
    });
  });

  await page.route(
    '**/api/tasks/0198f652-0000-7000-8000-000000000014',
    async (route) => {
      await fulfillJson(route, 200, {
        task: tasks[3],
        attempts: [
          {
            attempt_number: 1,
            state: 'waiting_storage',
            started_at: '2026-07-30T02:40:00Z',
            finished_at: '2026-07-30T02:40:01Z',
            error_class: null,
            retryable: null,
            message: null,
            trace_id: null
          }
        ]
      });
    }
  );

  await page.route(`**/api/tasks/${failedId}/retry`, async (route) => {
    state.retryBody = route.request().postDataJSON();
    tasks[0] = { ...tasks[0], state: 'queued', revision: 7 };
    await fulfillJson(route, 200, tasks[0]);
  });

  await page.route(`**/api/tasks/${waitingId}/cancel`, async (route) => {
    state.cancelBody = route.request().postDataJSON();
    tasks[1] = { ...tasks[1], state: 'cancelled', revision: 5 };
    await fulfillJson(route, 200, tasks[1]);
  });

  return state;
}

function task(
  id: string,
  kind: string,
  state: string,
  errorClass: string | null,
  revision: number
) {
  return {
    id,
    priority:
      kind === 'ranking_collection'
        ? 'scheduled_collection'
        : 'background_maintenance',
    kind,
    state,
    attempts: state === 'running' ? 1 : 2,
    available_at: '2026-07-30T03:00:00Z',
    error_class: errorClass,
    retryable: errorClass === 'network' ? true : null,
    next_retry_at: errorClass === 'network' ? '2026-07-30T03:05:00Z' : null,
    revision,
    created_at: '2026-07-30T02:00:00Z',
    updated_at: '2026-07-30T02:30:00Z'
  };
}
