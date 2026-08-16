import { expect, test, type Page } from '@playwright/test';

import { chooseSelectOption, fulfillJson, mockApi } from './support';

const downloadRuleId = '0198f64c-42a2-7374-bace-9f1c3b317fb4';
const ugoiraRuleId = '0198f64c-42a2-7374-bace-9f1c3b317fb5';
const copiedRuleId = '0198f64c-42a2-7374-bace-9f1c3b317fb6';

test('rules workbench edits catalog fields and saves a version', async ({
  page
}) => {
  const api = await mockRules(page);
  await page.goto('/rules');

  await expect(page.getByRole('heading', { name: '规则工作台' })).toBeVisible();
  await expect(page.getByRole('region', { name: '规则列表' })).toBeVisible();
  await expect(page.getByRole('region', { name: '规则编辑器' })).toBeVisible();
  await expect(page.getByRole('region', { name: '判断过程' })).toBeVisible();

  await page.getByRole('button', { name: '选择规则 动图只记录元数据' }).click();
  await page
    .getByRole('switch', { name: '启用规则 动图只记录元数据' })
    .uncheck();
  await page.getByRole('button', { name: '规则操作 动图只记录元数据' }).click();
  await page.getByRole('button', { name: '重命名', exact: true }).click();
  await page.getByLabel('重命名规则 动图只记录元数据').fill('高质量动图');
  await page.getByLabel('重命名规则 动图只记录元数据').press('Enter');
  await chooseSelectOption(page, '规则命中动作', '下载原图');
  await page.getByRole('button', { name: '添加条件组' }).click();
  await chooseSelectOption(page, '条件组2模式', '组内任一满足');
  await chooseSelectOption(page, '条件组2条件1字段', '作品 · 标签');
  await chooseSelectOption(page, '条件组2条件1运算符', '包含全部标签');
  await expect(page.getByLabel('条件组2条件1值')).toHaveAttribute(
    'placeholder',
    '例如：猫, original'
  );
  await page.getByLabel('条件组2条件1值').fill('原创, 风景');

  await chooseSelectOption(page, '条件组2条件1运算符', '有值');
  await expect(page.getByLabel('条件组2条件1值')).toHaveCount(0);
  await expect(page.locator('.no-value').last()).toBeVisible();
  await expect(page.locator('.no-value').last()).toBeEmpty();
  await expect(page.getByText('无需填写')).toHaveCount(0);
  await expect(page.getByText('此运算符不需要填写值')).toHaveCount(0);

  await expect(page.getByText('草稿已保存')).toBeVisible();
  await page.getByRole('button', { name: '保存', exact: true }).click();
  await expect(page.getByText('已保存', { exact: true })).toBeVisible();

  expect(api.published).toHaveLength(1);
  expect(api.published[0]).toMatchObject({
    id: ugoiraRuleId,
    name: '高质量动图',
    enabled: false,
    action: 'download',
    group_mode: 'all'
  });
  expect(api.published[0].groups).toHaveLength(2);
  expect(api.published[0].groups[1].conditions[0]).toEqual({
    field: 'tags',
    operator: 'exists',
    case_sensitive: false,
    tag_scope: 'original_and_translation'
  });
});

test('rules workbench creates the first rule from the add button', async ({
  page
}) => {
  const api = await mockEmptyRules(page, { deferCreate: true });
  await page.goto('/rules');

  await expect(page.getByRole('region', { name: '规则列表' })).toBeVisible();
  await expect(page.getByRole('button', { name: '新建规则' })).toBeVisible();

  await page.getByRole('button', { name: '新建规则' }).click();
  const ruleName = page.getByLabel('规则名称');
  await expect(ruleName).toHaveCSS('font-size', '12.8px');
  await expect(ruleName).toHaveCSS('font-weight', '500');
  await ruleName.fill('新规则1');
  await page.getByRole('button', { name: '创建规则' }).click();
  await expect(page.getByRole('button', { name: '正在创建' })).toBeDisabled();
  api.finishCreate();

  await expect(
    page.getByRole('button', { name: '选择规则 新规则1' })
  ).toBeVisible();
  await expect(
    page.getByRole('heading', { name: '新规则1', exact: true })
  ).toBeVisible();
  expect(api.created).toEqual([
    { name: '新规则1', default_action: 'download' }
  ]);
});

test('finite rule fields use catalog selectors and dates use the shared picker', async ({
  page
}) => {
  await mockRules(page);
  await page.goto('/rules');
  await page.getByRole('button', { name: '选择规则 动图只记录元数据' }).click();

  const contentTypeValue = page.getByRole('button', {
    name: '条件组1条件1值'
  });
  await expect(contentTypeValue).toContainText('动图');
  await expect(
    page.getByText('作品在Pixiv中的内容类型；可选值：插画、漫画、动图')
  ).toBeVisible();
  await contentTypeValue.click();
  await expect(page.getByRole('option', { name: '插画' })).toBeVisible();
  await expect(page.getByRole('option', { name: '漫画' })).toBeVisible();
  await expect(page.getByRole('option', { name: '动图' })).toBeVisible();
  await page.keyboard.press('Escape');

  await chooseSelectOption(page, '条件组1条件1运算符', '属于任一分类');
  const contentTypes = page.getByRole('button', {
    name: '条件组1条件1值'
  });
  await expect(contentTypes).toContainText('插画');
  await contentTypes.click();
  await page.getByRole('option', { name: '漫画' }).click();
  await page.keyboard.press('Escape');
  await expect(contentTypes).toContainText('插画');
  await expect(contentTypes).toContainText('漫画');

  await chooseSelectOption(page, '条件组1条件1字段', '作品 · 标题');
  const titleValue = page.getByLabel('条件组1条件1值');
  await expect(titleValue).toHaveAttribute('type', 'text');
  await titleValue.fill('夏日');

  await chooseSelectOption(page, '条件组1条件1字段', '媒体 · 原始扩展名');
  const extensionValue = page.getByRole('button', {
    name: '条件组1条件1值'
  });
  await extensionValue.click();
  await expect(page.getByRole('option', { name: 'JPEG' })).toBeVisible();
  await expect(page.getByRole('option', { name: 'PNG' })).toBeVisible();
  await expect(page.getByRole('option', { name: 'GIF' })).toBeVisible();
  await page.keyboard.press('Escape');

  await chooseSelectOption(page, '条件组1条件1字段', '时间 · 发布时间');
  const dateTime = page.getByRole('group', { name: '条件组1条件1值' });
  await expect(dateTime).toBeVisible();
  await expect(page.locator('input[type="datetime-local"]')).toHaveCount(0);

  await dateTime.getByRole('button', { name: '选择条件组1条件1值' }).click();
  await page.getByRole('button', { name: '条件组1条件1值年份' }).click();
  await page.getByRole('option', { name: '2025年' }).click();
  await page.getByRole('button', { name: '条件组1条件1值月份' }).click();
  await page.getByRole('option', { name: '6月' }).click();
  await page.getByRole('button', { name: /2025年6月2日/ }).click();
  const selectedDay = page.locator('.pa-calendar-day[data-selected]');
  await expect(selectedDay).toBeVisible();
  expect(
    await selectedDay.evaluate((element) => {
      const style = getComputedStyle(element);
      return {
        alignItems: style.alignItems,
        justifyItems: style.justifyItems
      };
    })
  ).toEqual({ alignItems: 'center', justifyItems: 'center' });
  await page.getByRole('button', { name: '条件组1条件1值小时' }).click();
  await page.getByRole('option', { name: '13时' }).click();
  await page.getByRole('button', { name: '条件组1条件1值分钟' }).click();
  await page.getByRole('option', { name: '45分' }).click();
  await page.getByRole('button', { name: '完成日期时间选择' }).click();

  await expect(dateTime.locator('[data-segment="year"]')).toHaveText('2025');
  await expect(dateTime.locator('[data-segment="month"]')).toHaveText('06');
  await expect(dateTime.locator('[data-segment="day"]')).toHaveText('02');
  await expect(dateTime.locator('[data-segment="hour"]')).toHaveText('13');
  await expect(dateTime.locator('[data-segment="minute"]')).toHaveText('45');
});

test('rule date-times follow the browser timezone and submit UTC instants', async ({
  browser
}) => {
  const cases = [
    {
      timezoneId: 'Asia/Shanghai',
      displayedHour: '20',
      submittedValue: '2026-08-25T02:30:00.000Z'
    },
    {
      timezoneId: 'Europe/Amsterdam',
      displayedHour: '14',
      submittedValue: '2026-08-25T08:30:00.000Z'
    }
  ];

  for (const current of cases) {
    const context = await browser.newContext({
      timezoneId: current.timezoneId
    });
    const page = await context.newPage();
    try {
      const api = await mockRules(page);
      api.current.set(
        ugoiraRuleId,
        dateRule(ugoiraDefinition(), '2026-07-01T12:00:00Z')
      );
      await page.goto('/rules');
      await page
        .getByRole('button', { name: '选择规则 动图只记录元数据' })
        .click();

      const dateTime = page.getByRole('group', { name: '条件组1条件1值' });
      await expect(dateTime.locator('[data-segment="year"]')).toHaveText(
        '2026'
      );
      await expect(dateTime.locator('[data-segment="month"]')).toHaveText('07');
      await expect(dateTime.locator('[data-segment="day"]')).toHaveText('01');
      await expect(dateTime.locator('[data-segment="hour"]')).toHaveText(
        current.displayedHour
      );

      await selectLocalRuleDateTime(page, 2026, 8, 25, 10, 30);
      await expect
        .poll(() => dateConditionValue(api.current.get(ugoiraRuleId)))
        .toBe(current.submittedValue);
    } finally {
      await context.close();
    }
  }
});

test('Amsterdam time display follows the daylight-saving jump', async ({
  browser
}) => {
  const context = await browser.newContext({
    timezoneId: 'Europe/Amsterdam'
  });
  const page = await context.newPage();
  try {
    const api = await mockRules(page);
    api.current.set(
      downloadRuleId,
      dateRule(downloadDefinition(), '2026-03-29T00:30:00Z')
    );
    api.current.set(
      ugoiraRuleId,
      dateRule(ugoiraDefinition(), '2026-03-29T01:30:00Z')
    );
    await page.goto('/rules');

    await page.getByRole('button', { name: '选择规则 收藏数至少500' }).click();
    await expect(
      page
        .getByRole('group', { name: '条件组1条件1值' })
        .locator('[data-segment="hour"]')
    ).toHaveText('01');

    await page
      .getByRole('button', { name: '选择规则 动图只记录元数据' })
      .click();
    await expect(
      page
        .getByRole('group', { name: '条件组1条件1值' })
        .locator('[data-segment="hour"]')
    ).toHaveText('03');
  } finally {
    await context.close();
  }
});

test('rules import and export reject unsupported operators', async ({
  page
}) => {
  const api = await mockRules(page);
  await page.goto('/rules');

  await page.getByRole('button', { name: '规则操作 收藏数至少500' }).click();
  const download = page.waitForEvent('download');
  await page.getByRole('button', { name: '导出JSON' }).click();
  expect((await download).suggestedFilename()).toBe('收藏数至少500.rule.json');

  await page.getByRole('button', { name: '规则操作 收藏数至少500' }).click();
  await page.getByRole('button', { name: '导入JSON' }).click();
  const invalid = downloadDefinition();
  invalid.groups[0].conditions = [
    {
      field: 'title',
      operator: 'regex',
      value: { type: 'text', value: '.*' }
    }
  ];
  await page.getByLabel('规则JSON').fill(JSON.stringify(invalid));
  await page.getByRole('button', { name: '验证并导入' }).click();
  await expect(page.getByRole('alert')).toContainText('不受支持');
  expect(api.imported).toHaveLength(0);

  await page.getByLabel('规则JSON').fill(JSON.stringify(downloadDefinition()));
  await page.getByRole('button', { name: '验证并导入' }).click();
  await expect(page.getByText('规则JSON已载入草稿')).toBeVisible();
  expect(api.imported).toHaveLength(1);
});

test('rule search, copy, and drag order preview stay in one catalog', async ({
  page
}) => {
  const api = await mockRules(page);
  await page.goto('/rules');

  await page.getByLabel('搜索规则').fill('动图');
  await expect(
    page.getByRole('button', { name: '选择规则 收藏数至少500' })
  ).toHaveCount(0);
  await expect(
    page.getByRole('button', { name: '拖动规则 动图只记录元数据' })
  ).toBeDisabled();
  await page.getByLabel('搜索规则').fill('');

  await page.getByRole('button', { name: '规则操作 收藏数至少500' }).click();
  await page.getByRole('button', { name: '复制', exact: true }).click();
  await expect(
    page.getByRole('button', { name: '选择规则 收藏数至少500 副本' })
  ).toBeVisible();

  const cancelledTransfer = await page.evaluateHandle(() => new DataTransfer());
  const downloadHandle = page.getByRole('button', {
    name: '拖动规则 收藏数至少500',
    exact: true
  });
  const ugoiraRow = page.locator(`[data-rule-id="${ugoiraRuleId}"]`);
  const ugoiraBox = await ugoiraRow.boundingBox();
  expect(ugoiraBox).not.toBeNull();

  await downloadHandle.dispatchEvent('dragstart', {
    dataTransfer: cancelledTransfer
  });
  await ugoiraRow.dispatchEvent('dragover', {
    clientY: ugoiraBox!.y + ugoiraBox!.height * 0.75,
    dataTransfer: cancelledTransfer
  });
  await expect(page.locator('[data-rule-id]').first()).toHaveAttribute(
    'data-rule-id',
    ugoiraRuleId
  );
  await downloadHandle.dispatchEvent('dragend', {
    dataTransfer: cancelledTransfer
  });
  await expect(page.locator('[data-rule-id]').first()).toHaveAttribute(
    'data-rule-id',
    downloadRuleId
  );
  await expect
    .poll(() =>
      page
        .locator('.rule-order')
        .evaluate((element) =>
          element
            .getAnimations({ subtree: true })
            .some((animation) => animation.playState === 'running')
        )
    )
    .toBe(false);
  expect(api.orderedRuleIds).toEqual([]);

  const dataTransfer = await page.evaluateHandle(() => new DataTransfer());
  const ugoiraHandle = page.getByRole('button', {
    name: '拖动规则 动图只记录元数据'
  });
  const downloadRow = page.locator(`[data-rule-id="${downloadRuleId}"]`);
  const downloadBox = await downloadRow.boundingBox();
  expect(downloadBox).not.toBeNull();

  await page.locator('.rule-order').evaluate((ruleOrder) => {
    const animationWindow = window as typeof window & {
      __ruleFlipAnimationCount?: number;
    };
    const originalAnimate = Element.prototype.animate;
    animationWindow.__ruleFlipAnimationCount = 0;
    Element.prototype.animate = function (
      keyframes: Keyframe[] | PropertyIndexedKeyframes | null,
      options?: number | KeyframeAnimationOptions
    ): Animation {
      const duration =
        typeof options === 'number'
          ? options
          : typeof options?.duration === 'number'
            ? options.duration
            : 0;
      if (duration > 0 && ruleOrder.contains(this)) {
        animationWindow.__ruleFlipAnimationCount =
          (animationWindow.__ruleFlipAnimationCount ?? 0) + 1;
      }
      return originalAnimate.call(this, keyframes, options);
    };
  });

  await ugoiraHandle.dispatchEvent('dragstart', { dataTransfer });
  await downloadRow.dispatchEvent('dragover', {
    clientY: downloadBox!.y + downloadBox!.height * 0.25,
    dataTransfer
  });

  expect(api.orderedRuleIds).toEqual([]);
  await expect
    .poll(() =>
      page.evaluate(
        () =>
          (
            window as typeof window & {
              __ruleFlipAnimationCount?: number;
            }
          ).__ruleFlipAnimationCount ?? 0
      )
    )
    .toBeGreaterThan(0);
  await expect(page.locator('[data-rule-id]').first()).toHaveAttribute(
    'data-rule-id',
    ugoiraRuleId
  );

  await downloadRow.dispatchEvent('drop', { dataTransfer });
  await ugoiraHandle.dispatchEvent('dragend', { dataTransfer });
  await expect
    .poll(() => api.orderedRuleIds)
    .toEqual([ugoiraRuleId, downloadRuleId, api.copiedRuleId]);

  await page.reload();
  await expect(page.locator('[data-rule-id]').first()).toHaveAttribute(
    'data-rule-id',
    ugoiraRuleId
  );
});

test('judgment trace validates a Pixiv work PID and has no batch trial', async ({
  page
}) => {
  await mockRules(page);
  await page.goto('/rules');

  await page.getByLabel('作品PID').fill('abc');
  await page.getByRole('button', { name: '检查作品' }).click();
  await expect(page.getByRole('alert')).toContainText('数字作品PID');

  await page.getByLabel('作品PID').fill('120001');
  await page.getByRole('button', { name: '检查作品' }).click();
  await expect(page.getByText('命中：收藏数至少500')).toBeVisible();
  await expect(
    page
      .getByRole('region', { name: '判断过程' })
      .getByText('下载原图', { exact: true })
  ).toBeVisible();
  await expect(page.getByText('Pixiv ID 120001')).toBeVisible();
  await expect(page.getByText('打开批量试跑')).toHaveCount(0);
});

test('narrow rules workbench switches between its three views', async ({
  page
}) => {
  await mockRules(page);
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto('/rules');

  await expect(page.getByRole('region', { name: '规则列表' })).toBeVisible();
  await page.getByRole('button', { name: '编辑器视图' }).click();
  await expect(page.getByRole('region', { name: '规则编辑器' })).toBeVisible();
  await page.getByRole('button', { name: '测试结果视图' }).click();
  await expect(page.getByRole('region', { name: '判断过程' })).toBeVisible();
});

async function mockRules(page: Page) {
  await mockApi(page);
  const state = {
    draftRevision: new Map([
      [downloadRuleId, 7],
      [ugoiraRuleId, 3]
    ]),
    current: new Map([
      [downloadRuleId, structuredClone(downloadDefinition())],
      [ugoiraRuleId, structuredClone(ugoiraDefinition())]
    ]),
    order: [downloadRuleId, ugoiraRuleId],
    published: [] as RuleFixture[],
    imported: [] as RuleFixture[],
    publishedVersion: new Map<string, number>(),
    copiedRuleId,
    orderedRuleIds: [] as string[]
  };

  const summary = (ruleId: string) => {
    const rule = state.current.get(ruleId)!;
    return {
      id: rule.id,
      name: rule.name,
      enabled: rule.enabled,
      action: rule.action,
      default_action: rule.default_action,
      current_version_id: '0198f64c-42a2-7374-bace-9f1c3b317fb1',
      current_version: state.publishedVersion.get(rule.id) ?? 2,
      lifecycle: state.publishedVersion.has(rule.id) ? 'published' : 'modified',
      revision: 4,
      sort_order: state.order.indexOf(ruleId) + 1
    };
  };

  await page.route('**/api/rules**', async (route) => {
    const request = route.request();
    const url = new URL(request.url());
    const path = url.pathname;
    const method = request.method();

    if (path === '/api/rules' && method === 'GET') {
      await fulfillJson(route, 200, {
        items: state.order.map(summary)
      });
      return;
    }

    if (path === '/api/rules/order' && method === 'PUT') {
      const body = request.postDataJSON();
      state.order = [...body.ordered_rule_ids];
      state.orderedRuleIds = [...state.order];
      await fulfillJson(route, 200, { items: state.order.map(summary) });
      return;
    }

    const ruleId = ruleIdFromPath(path);
    if (!ruleId || !state.current.has(ruleId)) {
      await route.continue();
      return;
    }
    const current = () => state.current.get(ruleId)!;

    if (path === `/api/rules/${ruleId}/copy` && method === 'POST') {
      const body = request.postDataJSON();
      const copied = {
        ...structuredClone(current()),
        id: copiedRuleId,
        name: body.name
      };
      state.current.set(copiedRuleId, copied);
      state.draftRevision.set(copiedRuleId, 1);
      state.order.push(copiedRuleId);
      await fulfillJson(route, 201, summary(copiedRuleId));
      return;
    }

    if (path === `/api/rules/${ruleId}/draft` && method === 'GET') {
      await fulfillJson(route, 200, {
        id: '0198f64c-42a2-7374-bace-9f1c3b317fb9',
        rule_id: ruleId,
        base_version: 2,
        schema_version: 1,
        definition: current(),
        revision: state.draftRevision.get(ruleId)
      });
      return;
    }
    if (path === `/api/rules/${ruleId}/draft` && method === 'PUT') {
      const body = request.postDataJSON();
      state.current.set(ruleId, structuredClone(body.definition));
      state.draftRevision.set(
        ruleId,
        (state.draftRevision.get(ruleId) ?? 0) + 1
      );
      await fulfillJson(route, 200, {
        id: '0198f64c-42a2-7374-bace-9f1c3b317fb9',
        rule_id: ruleId,
        base_version: body.base_version,
        schema_version: 1,
        definition: current(),
        revision: state.draftRevision.get(ruleId)
      });
      return;
    }
    if (path === `/api/rules/${ruleId}/publish` && method === 'POST') {
      state.published.push(structuredClone(current()));
      state.publishedVersion.set(ruleId, 3);
      await fulfillJson(route, 201, {
        id: '0198f64c-42a2-7374-bace-9f1c3b317fb8',
        rule_id: ruleId,
        version: 3,
        base_version: 2,
        schema_version: 1,
        definition: current(),
        created_by: '0198f64c-42a2-7374-bace-9f1c3b317fb7'
      });
      return;
    }
    if (path === `/api/rules/${ruleId}/export` && method === 'GET') {
      await fulfillJson(route, 200, current());
      return;
    }
    if (path === `/api/rules/${ruleId}/import` && method === 'PUT') {
      const body = request.postDataJSON();
      if (JSON.stringify(body.definition).includes('"regex"')) {
        await fulfillJson(route, 422, {
          code: 'invalid_request',
          message: 'unknown variant `regex`',
          details: {},
          trace_id: '0198f64c-42a2-7374-bace-9f1c3b317fbe'
        });
        return;
      }
      state.current.set(ruleId, structuredClone(body.definition));
      state.imported.push(structuredClone(current()));
      state.draftRevision.set(
        ruleId,
        (state.draftRevision.get(ruleId) ?? 0) + 1
      );
      await fulfillJson(route, 200, {
        id: '0198f64c-42a2-7374-bace-9f1c3b317fbb',
        rule_id: ruleId,
        base_version: 2,
        schema_version: 1,
        definition: current(),
        revision: state.draftRevision.get(ruleId)
      });
      return;
    }
    if (path === `/api/rules/${ruleId}/preview` && method === 'POST') {
      const body = request.postDataJSON();
      await fulfillJson(route, 200, {
        item: previewItem(body.pixiv_work_id, current())
      });
      return;
    }
    await route.continue();
  });

  return state;
}

async function mockEmptyRules(
  page: Page,
  options: { deferCreate?: boolean } = {}
) {
  await mockApi(page);
  const createdRuleId = '0198f64c-42a2-7374-bace-9f1c3b317fc0';
  let finishCreate = () => {};
  const createGate = options.deferCreate
    ? new Promise<void>((resolve) => {
        finishCreate = resolve;
      })
    : Promise.resolve();
  const state = {
    created: [] as Array<{ name: string; default_action: string }>
  };
  const createdDefinition = createRuleDefinition(createdRuleId, '新规则1');

  await page.route('**/api/rules**', async (route) => {
    const request = route.request();
    const path = new URL(request.url()).pathname;
    const method = request.method();

    if (path === '/api/rules' && method === 'GET') {
      await fulfillJson(route, 200, { items: [] });
      return;
    }
    if (path === '/api/rules' && method === 'POST') {
      const body = request.postDataJSON();
      state.created.push(body);
      await createGate;
      await fulfillJson(route, 201, {
        id: createdRuleId,
        name: body.name,
        enabled: true,
        action: 'download',
        default_action: body.default_action,
        current_version_id: null,
        current_version: null,
        lifecycle: 'draft',
        revision: 1,
        sort_order: 1
      });
      return;
    }
    if (path === `/api/rules/${createdRuleId}/draft` && method === 'GET') {
      await fulfillJson(route, 200, {
        id: '0198f64c-42a2-7374-bace-9f1c3b317fc1',
        rule_id: createdRuleId,
        base_version: null,
        schema_version: 1,
        definition: createdDefinition,
        revision: 1
      });
      return;
    }
    await route.continue();
  });

  return { ...state, finishCreate };
}

function previewItem(pixivWorkId: number, current: RuleFixture) {
  const matched = current.id === downloadRuleId;
  return {
    pixiv_work_id: pixivWorkId,
    title: '高收藏插画',
    artist_name: '示例作者A',
    content_type: 'illustration',
    decision: matched ? current.action : current.default_action,
    matched_rule_id: matched ? current.id : null,
    trace: {
      decision: matched ? current.action : current.default_action,
      matched_rule_id: matched ? current.id : null,
      rules: [
        {
          rule_index: 0,
          rule_id: current.id,
          rule_name: current.name,
          action: current.action,
          group_mode: current.group_mode,
          state: matched ? 'matched' : 'not_matched',
          groups: current.groups.map((group, groupIndex) => ({
            group_index: groupIndex,
            mode: group.mode,
            result: matched,
            state: 'evaluated',
            conditions: group.conditions.map((condition, conditionIndex) => ({
              condition_index: conditionIndex,
              field: condition.field,
              operator: condition.operator,
              result: matched,
              state: 'evaluated',
              value: null,
              pages: [],
              stopped_at_page_index: null
            }))
          }))
        }
      ]
    }
  };
}

interface RuleFixture {
  schema_version: 1;
  id: string;
  name: string;
  enabled: boolean;
  group_mode: 'all' | 'any';
  groups: Array<{
    mode: 'all' | 'any';
    conditions: Array<Record<string, unknown>>;
  }>;
  action: 'download' | 'metadata_only' | 'ignore';
  default_action: 'download' | 'metadata_only' | 'ignore';
}

function downloadDefinition(): RuleFixture {
  return {
    schema_version: 1,
    id: downloadRuleId,
    name: '收藏数至少500',
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
    default_action: 'ignore'
  };
}

function ugoiraDefinition(): RuleFixture {
  return {
    schema_version: 1,
    id: ugoiraRuleId,
    name: '动图只记录元数据',
    enabled: true,
    group_mode: 'all',
    groups: [
      {
        mode: 'all',
        conditions: [
          {
            field: 'content_type',
            operator: 'equals',
            value: { type: 'text', value: 'ugoira' }
          }
        ]
      }
    ],
    action: 'metadata_only',
    default_action: 'download'
  };
}

function dateRule(definition: RuleFixture, value: string): RuleFixture {
  const updated = structuredClone(definition);
  updated.groups[0].conditions = [
    {
      field: 'published_at',
      operator: 'after',
      value: { type: 'date', value }
    }
  ];
  return updated;
}

function dateConditionValue(definition: RuleFixture | undefined): string {
  const condition = definition?.groups[0]?.conditions[0] as
    { value?: { type?: string; value?: string } } | undefined;
  return condition?.value?.value ?? '';
}

async function selectLocalRuleDateTime(
  page: Page,
  year: number,
  month: number,
  day: number,
  hour: number,
  minute: number
): Promise<void> {
  const label = '条件组1条件1值';
  await page
    .getByRole('group', { name: label })
    .getByRole('button', { name: `选择${label}` })
    .click();
  await page.getByRole('button', { name: `${label}年份` }).click();
  await page.getByRole('option', { name: `${year}年` }).click();
  await page.getByRole('button', { name: `${label}月份` }).click();
  await page.getByRole('option', { name: `${month}月` }).click();
  await page
    .getByRole('button', { name: new RegExp(`${year}年${month}月${day}日`) })
    .click();
  await page.getByRole('button', { name: `${label}小时` }).click();
  await page.getByRole('option', { name: `${hour}时` }).click();
  await page.getByRole('button', { name: `${label}分钟` }).click();
  await page.getByRole('option', { name: `${minute}分` }).click();
  await page.getByRole('button', { name: '完成日期时间选择' }).click();
}

function createRuleDefinition(id: string, name: string): RuleFixture {
  return {
    ...downloadDefinition(),
    id,
    name,
    default_action: 'download'
  };
}

function ruleIdFromPath(path: string): string | null {
  return path.match(/^\/api\/rules\/([^/]+)/)?.[1] ?? null;
}
