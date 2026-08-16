import { flushSync } from 'svelte';
import { describe, expect, it, vi } from 'vitest';

import {
  createRuleDefinition,
  type RuleDraft,
  type RulePreviewResponse,
  type RuleSummary,
  type RuleVersion,
  type RuleWorkbenchApi
} from '$lib/api/rules';
import { createRuleWorkbenchStore } from './rule-workbench.svelte';

const ruleA = summary('0198f64c-42a2-7374-bace-9f1c3b317fb0', '收藏筛选', 1);
const ruleB = summary('0198f64c-42a2-7374-bace-9f1c3b317fb1', '标签筛选', 2);
const ruleC = summary('0198f64c-42a2-7374-bace-9f1c3b317fb2', '作者筛选', 3);

describe('rule workbench store', () => {
  it('restores and loads one rule with condition groups directly beneath it', async () => {
    const { api } = fakeApi();
    const store = createRuleWorkbenchStore({
      api,
      storage: memoryStorage({ 'pixivarchive.rules.selectedRule': ruleB.id })
    });

    await store.initialize();

    expect(store.selectedRuleId).toBe(ruleB.id);
    expect(store.selectedRule?.name).toBe('标签筛选');
    expect(store.selectedRule?.groups).toHaveLength(1);
  });

  it('retries initialization after a catalog loading failure', async () => {
    const { api } = fakeApi();
    api.listRules = vi
      .fn<RuleWorkbenchApi['listRules']>()
      .mockRejectedValueOnce(new Error('catalog unavailable'))
      .mockResolvedValueOnce([structuredClone(ruleA), structuredClone(ruleB)]);
    const store = createRuleWorkbenchStore({ api, storage: memoryStorage() });

    await store.initialize();
    expect(store.initialized).toBe(false);
    expect(store.loadError).toBe('规则暂时无法读取');

    await store.initialize();

    expect(api.listRules).toHaveBeenCalledTimes(2);
    expect(store.initialized).toBe(true);
    expect(store.selectedRuleId).toBe(ruleA.id);
    expect(store.loadError).toBe('');
  });

  it('retries initialization after the selected draft cannot be loaded', async () => {
    const { api } = fakeApi();
    let attempts = 0;
    api.loadDraft = vi.fn(async (ruleId) => {
      attempts += 1;
      if (attempts === 1) throw new Error('draft unavailable');
      return draft(ruleId, ruleA.name);
    });
    const store = createRuleWorkbenchStore({ api, storage: memoryStorage() });

    await store.initialize();
    expect(store.initialized).toBe(false);
    expect(store.loadError).toBe('规则暂时无法读取');

    await store.initialize();

    expect(api.listRules).toHaveBeenCalledTimes(2);
    expect(api.loadDraft).toHaveBeenCalledTimes(2);
    expect(store.initialized).toBe(true);
    expect(store.selectedRuleId).toBe(ruleA.id);
    expect(store.loadError).toBe('');
  });

  it('shares one catalog load between concurrent initialization calls', async () => {
    const { api } = fakeApi();
    const rules = deferred<RuleSummary[]>();
    api.listRules = vi.fn(async () => rules.promise);
    const store = createRuleWorkbenchStore({ api, storage: memoryStorage() });

    const first = store.initialize();
    const second = store.initialize();

    expect(api.listRules).toHaveBeenCalledTimes(1);
    rules.resolve([structuredClone(ruleA), structuredClone(ruleB)]);
    await Promise.all([first, second]);

    expect(store.initialized).toBe(true);
    expect(store.selectedRuleId).toBe(ruleA.id);
    expect(store.loadError).toBe('');
  });

  it('refreshes the catalog and keeps the selected rule on its latest server document', async () => {
    const { api } = fakeApi();
    const refreshedRuleA = {
      ...structuredClone(ruleA),
      name: '服务端更新后的收藏筛选',
      revision: ruleA.revision + 1
    };
    api.listRules = vi
      .fn<RuleWorkbenchApi['listRules']>()
      .mockResolvedValueOnce([structuredClone(ruleA), structuredClone(ruleB)])
      .mockResolvedValueOnce([
        refreshedRuleA,
        structuredClone(ruleB),
        structuredClone(ruleC)
      ]);
    api.loadDraft = vi
      .fn<RuleWorkbenchApi['loadDraft']>()
      .mockResolvedValueOnce(draft(ruleA.id, ruleA.name))
      .mockResolvedValueOnce(draft(ruleA.id, refreshedRuleA.name));
    const store = createRuleWorkbenchStore({ api, storage: memoryStorage() });
    await store.initialize();

    expect(await store.refresh()).toBe(true);

    expect(store.rules.map((rule) => rule.id)).toEqual([
      ruleA.id,
      ruleB.id,
      ruleC.id
    ]);
    expect(store.selectedRuleId).toBe(ruleA.id);
    expect(store.document?.name).toBe(refreshedRuleA.name);
    expect(store.loadError).toBe('');
  });

  it('does not replace a dirty rule when saving it before refresh fails', async () => {
    const { api } = fakeApi();
    api.saveDraft = vi.fn(async () => {
      throw new Error('save unavailable');
    });
    const store = createRuleWorkbenchStore({
      api,
      storage: memoryStorage(),
      autosaveMs: 60_000
    });
    await store.initialize();
    store.renameRule(ruleA.id, '尚未保存的名称');

    expect(await store.refresh()).toBe(false);

    expect(api.listRules).toHaveBeenCalledTimes(1);
    expect(store.selectedRuleId).toBe(ruleA.id);
    expect(store.document?.name).toBe('尚未保存的名称');
    expect(store.loadError).toBe('当前规则保存失败，请重试');
  });

  it('creates a rule and selects its server-created draft', async () => {
    const { api } = fakeApi();
    api.createRule = vi.fn(async () => ruleB);
    api.loadDraft = vi.fn(async (id) => draft(id, '标签筛选'));
    const store = createRuleWorkbenchStore({ api, storage: memoryStorage() });

    await store.initialize();
    expect(await store.createRule(' 标签筛选 ')).toBe(true);

    expect(api.createRule).toHaveBeenCalledWith('标签筛选', 'download');
    expect(store.selectedRuleId).toBe(ruleB.id);
  });

  it('filters the catalog by rule name without changing its order', async () => {
    const { api } = fakeApi();
    const store = createRuleWorkbenchStore({ api, storage: memoryStorage() });
    await store.initialize();

    store.ruleSearch = ' 标签 ';

    expect(store.visibleRules.map((rule) => rule.id)).toEqual([ruleB.id]);
    expect(store.rules.map((rule) => rule.id)).toEqual([ruleA.id, ruleB.id]);
  });

  it('copies a rule and selects the server-created copy', async () => {
    const { api } = fakeApi();
    api.copyRule = vi.fn(async (_ruleId, request) => ({
      ...structuredClone(ruleC),
      name: request.name
    }));
    api.loadDraft = vi.fn(async (ruleId) =>
      draft(ruleId, ruleId === ruleC.id ? '收藏筛选 副本' : ruleA.name)
    );
    const store = createRuleWorkbenchStore({ api, storage: memoryStorage() });
    await store.initialize();

    expect(await store.copyRule(ruleA.id)).toBe(true);

    expect(api.copyRule).toHaveBeenCalledWith(ruleA.id, {
      name: '收藏筛选 副本'
    });
    expect(store.rules.map((rule) => rule.id)).toEqual([
      ruleA.id,
      ruleB.id,
      ruleC.id
    ]);
    expect(store.selectedRuleId).toBe(ruleC.id);
  });

  it('selects and saves a renamed catalog rule', async () => {
    const { api } = fakeApi();
    const store = createRuleWorkbenchStore({ api, storage: memoryStorage() });
    await store.initialize();

    expect(await store.updateRuleName(ruleB.id, '风景标签')).toBe(true);

    expect(store.selectedRuleId).toBe(ruleB.id);
    expect(api.saveDraft).toHaveBeenCalledWith(
      ruleB.id,
      expect.objectContaining({
        definition: expect.objectContaining({ name: '风景标签' })
      })
    );
  });

  it('selects and saves an enabled-state change from the catalog', async () => {
    const { api } = fakeApi();
    const store = createRuleWorkbenchStore({ api, storage: memoryStorage() });
    await store.initialize();

    expect(await store.updateRuleEnabled(ruleB.id, false)).toBe(true);

    expect(store.selectedRuleId).toBe(ruleB.id);
    expect(api.saveDraft).toHaveBeenCalledWith(
      ruleB.id,
      expect.objectContaining({
        definition: expect.objectContaining({ enabled: false })
      })
    );
  });

  it('persists a complete reordered catalog', async () => {
    const { api } = fakeApi();
    api.reorderRules = vi.fn(async () => [
      { ...structuredClone(ruleB), sort_order: 1 },
      { ...structuredClone(ruleA), sort_order: 2 }
    ]);
    const store = createRuleWorkbenchStore({ api, storage: memoryStorage() });
    await store.initialize();

    expect(await store.reorderRules([ruleB.id, ruleA.id])).toBe(true);

    expect(api.reorderRules).toHaveBeenCalledWith({
      ordered_rule_ids: [ruleB.id, ruleA.id]
    });
    expect(store.rules.map((rule) => rule.id)).toEqual([ruleB.id, ruleA.id]);
  });

  it('reloads the server catalog when reordering fails', async () => {
    const { api } = fakeApi();
    api.listRules = vi
      .fn<RuleWorkbenchApi['listRules']>()
      .mockResolvedValueOnce([structuredClone(ruleA), structuredClone(ruleB)])
      .mockResolvedValueOnce([structuredClone(ruleA), structuredClone(ruleB)]);
    api.reorderRules = vi.fn(async () => {
      throw new Error('catalog changed');
    });
    const store = createRuleWorkbenchStore({ api, storage: memoryStorage() });
    await store.initialize();

    expect(await store.reorderRules([ruleB.id, ruleA.id])).toBe(false);

    expect(api.listRules).toHaveBeenCalledTimes(2);
    expect(store.rules.map((rule) => rule.id)).toEqual([ruleA.id, ruleB.id]);
    expect(store.catalogError).toBe('规则顺序暂时无法保存');
  });

  it('keeps a created rule when its draft cannot be loaded', async () => {
    const { api } = fakeApi();
    api.listRules = vi.fn(async () => [structuredClone(ruleA)]);
    api.createRule = vi.fn(async () => structuredClone(ruleC));
    api.loadDraft = vi.fn(async (id) => {
      if (id === ruleC.id) throw new Error('draft unavailable');
      return draft(id, ruleA.name);
    });
    const store = createRuleWorkbenchStore({ api, storage: memoryStorage() });

    await store.initialize();

    expect(await store.createRule('作者筛选')).toBe(true);
    expect(store.rules.map((rule) => rule.id)).toEqual([ruleA.id, ruleC.id]);
    expect(store.selectedRuleId).toBe(ruleA.id);
    expect(store.loadError).toBe('规则已创建，但内容暂时无法读取');
    expect(store.createRuleError).toBe('');
  });

  it('keeps the current document read only while creating another rule', async () => {
    const { api } = fakeApi();
    const created = deferred<RuleSummary>();
    api.createRule = vi.fn(async () => created.promise);
    const store = createRuleWorkbenchStore({ api, storage: memoryStorage() });
    await store.initialize();

    const creating = store.createRule('作者筛选');
    await vi.waitFor(() => expect(api.createRule).toHaveBeenCalledOnce());

    expect(store.documentReadOnly).toBe(true);
    store.renameRule(ruleA.id, '创建期间的编辑');
    expect(store.document?.name).toBe(ruleA.name);

    created.resolve(structuredClone(ruleC));
    expect(await creating).toBe(true);
    expect(store.documentReadOnly).toBe(false);
  });

  it('autosaves edits to the selected rule draft', async () => {
    vi.useFakeTimers();
    const { api } = fakeApi();
    const store = createRuleWorkbenchStore({
      api,
      storage: memoryStorage(),
      autosaveMs: 10
    });
    await store.initialize();

    store.renameRule(ruleA.id, '收藏数至少2000');
    flushSync();
    await vi.advanceTimersByTimeAsync(10);

    expect(api.saveDraft).toHaveBeenCalledWith(
      ruleA.id,
      expect.objectContaining({
        definition: expect.objectContaining({ name: '收藏数至少2000' })
      })
    );
    vi.useRealTimers();
  });

  it('keeps compatible values when a condition field or operator changes', async () => {
    const { api } = fakeApi();
    const store = createRuleWorkbenchStore({
      api,
      storage: memoryStorage(),
      autosaveMs: 60_000
    });
    await store.initialize();

    store.changeConditionValue(ruleA.id, 0, 0, {
      type: 'number',
      value: 3200
    });
    store.changeConditionOperator(ruleA.id, 0, 0, 'less_than');
    expect(store.document?.groups[0].conditions[0].value).toEqual({
      type: 'number',
      value: 3200
    });

    store.changeConditionField(ruleA.id, 0, 0, 'view_count');
    expect(store.document?.groups[0].conditions[0].value).toEqual({
      type: 'number',
      value: 3200
    });

    store.changeConditionOperator(ruleA.id, 0, 0, 'between');
    expect(store.document?.groups[0].conditions[0].value).toEqual({
      type: 'number_range',
      value: { min: 0, max: 0 }
    });

    store.changeConditionOperator(ruleA.id, 0, 0, 'exists');
    expect(store.document?.groups[0].conditions[0]).not.toHaveProperty('value');
  });

  it('updates the rule catalog from a saved draft', async () => {
    vi.useFakeTimers();
    const { api } = fakeApi();
    const store = createRuleWorkbenchStore({
      api,
      storage: memoryStorage(),
      autosaveMs: 10
    });
    await store.initialize();

    store.renameRule(ruleA.id, '收藏数至少2000');
    store.setRuleEnabled(ruleA.id, false);
    store.setRuleAction(ruleA.id, 'ignore');
    store.setDefaultAction('metadata_only');
    await vi.advanceTimersByTimeAsync(10);

    expect(store.selectedRuleSummary).toMatchObject({
      name: '收藏数至少2000',
      enabled: false,
      action: 'ignore',
      default_action: 'metadata_only',
      lifecycle: 'modified',
      revision: ruleA.revision + 1
    });
    vi.useRealTimers();
  });

  it('serializes slow autosaves and saves the latest edit with the new revision', async () => {
    vi.useFakeTimers();
    const { api } = fakeApi();
    const firstSave = deferred<RuleDraft>();
    api.saveDraft = vi
      .fn<RuleWorkbenchApi['saveDraft']>()
      .mockImplementationOnce(async () => firstSave.promise)
      .mockImplementationOnce(async (ruleId, request) => ({
        id: '0198f64c-42a2-7374-bace-9f1c3b317fbc',
        rule_id: ruleId,
        base_version: request.base_version ?? null,
        schema_version: 1,
        definition: structuredClone(request.definition),
        revision: 9
      }));
    const store = createRuleWorkbenchStore({
      api,
      storage: memoryStorage(),
      autosaveMs: 10
    });
    await store.initialize();

    store.renameRule(ruleA.id, '第一次编辑');
    flushSync();
    await vi.advanceTimersByTimeAsync(10);
    store.renameRule(ruleA.id, '第二次编辑');
    flushSync();
    firstSave.resolve({
      ...draft(ruleA.id, '第一次编辑'),
      revision: 8
    });
    await vi.runAllTimersAsync();

    expect(api.saveDraft).toHaveBeenCalledTimes(2);
    expect(api.saveDraft).toHaveBeenLastCalledWith(
      ruleA.id,
      expect.objectContaining({
        expected_revision: 8,
        definition: expect.objectContaining({ name: '第二次编辑' })
      })
    );
    expect(store.saveState).toBe('saved');
    vi.useRealTimers();
  });

  it('publishes the current draft and refreshes lifecycle state', async () => {
    const { api } = fakeApi();
    const store = createRuleWorkbenchStore({ api, storage: memoryStorage() });
    await store.initialize();

    await store.publish();

    expect(api.publishRule).toHaveBeenCalledWith(ruleA.id, {
      base_version: 2,
      expected_draft_revision: 7
    });
    expect(store.publishNotice).toBe('已保存');
  });

  it('keeps the published document read only until publication finishes', async () => {
    const { api } = fakeApi();
    const publication = deferred<RuleVersion>();
    api.publishRule = vi.fn(async () => publication.promise);
    const store = createRuleWorkbenchStore({ api, storage: memoryStorage() });
    await store.initialize();

    const publishing = store.publish();
    await vi.waitFor(() => expect(api.publishRule).toHaveBeenCalledOnce());

    expect(store.documentReadOnly).toBe(true);
    store.renameRule(ruleA.id, '发布期间的编辑');
    expect(store.document?.name).toBe(ruleA.name);

    publication.resolve(version(ruleA.id));
    await publishing;
    expect(store.documentReadOnly).toBe(false);
  });

  it('does not apply a publication response after another rule is selected', async () => {
    const { api } = fakeApi();
    const publication = deferred<RuleVersion>();
    api.publishRule = vi.fn(async () => publication.promise);
    const store = createRuleWorkbenchStore({ api, storage: memoryStorage() });
    await store.initialize();

    const publishing = store.publish();
    await vi.waitFor(() => expect(api.publishRule).toHaveBeenCalledOnce());
    await store.selectRule(ruleB.id);
    store.rules = [...store.rules, ruleC];
    publication.resolve(version(ruleA.id));
    await publishing;

    expect(store.selectedRuleId).toBe(ruleB.id);
    expect(store.selectedRule?.id).toBe(ruleB.id);
    expect(store.publishNotice).toBe('');
    expect(store.rules).toContainEqual(ruleC);
    expect(store.rules.find((rule) => rule.id === ruleA.id)).toMatchObject({
      current_version: 3,
      lifecycle: 'published',
      revision: ruleA.revision + 1
    });
  });

  it('keeps the successful publication when the catalog refresh fails', async () => {
    const { api } = fakeApi();
    api.listRules = vi
      .fn<RuleWorkbenchApi['listRules']>()
      .mockResolvedValueOnce([structuredClone(ruleA), structuredClone(ruleB)])
      .mockRejectedValueOnce(new Error('catalog unavailable'));
    const store = createRuleWorkbenchStore({ api, storage: memoryStorage() });
    await store.initialize();

    await store.publish();

    expect(store.publishError).toBe('');
    expect(store.publishNotice).toBe('已保存');
    expect(store.draftRevision).toBeNull();
    expect(store.baseVersion).toBe(3);
    expect(store.rules.find((rule) => rule.id === ruleA.id)).toMatchObject({
      current_version: 3,
      lifecycle: 'published',
      revision: ruleA.revision + 1
    });
  });

  it('does not apply an old preview response to the selected rule', async () => {
    const { api } = fakeApi();
    const preview = deferred<RulePreviewResponse>();
    api.previewRules = vi.fn(async () => preview.promise);
    const store = createRuleWorkbenchStore({ api, storage: memoryStorage() });
    await store.initialize();

    const previewing = store.preview(7101);
    await Promise.resolve();
    await store.selectRule(ruleB.id);
    preview.resolve({
      item: {
        pixiv_work_id: 7101,
        title: '旧规则作品',
        artist_name: '测试作者',
        content_type: 'illustration',
        decision: 'download',
        matched_rule_id: ruleA.id,
        trace: { decision: 'download', matched_rule_id: ruleA.id, rules: [] }
      }
    });
    await previewing;

    expect(store.selectedRuleId).toBe(ruleB.id);
    expect(store.previewItem).toBeNull();
  });

  it('clears a judgment preview when its rule document changes', async () => {
    const { api } = fakeApi();
    const store = createRuleWorkbenchStore({ api, storage: memoryStorage() });
    await store.initialize();
    await store.preview(7101);
    expect(store.previewItem?.pixiv_work_id).toBe(7101);

    store.renameRule(ruleA.id, '修改后的规则');

    expect(store.previewItem).toBeNull();
    expect(store.previewError).toBe('');
  });

  it('allows only one JSON import for a draft revision', async () => {
    const { api } = fakeApi();
    const imported = deferred<RuleDraft>();
    api.importRule = vi.fn(async () => imported.promise);
    const store = createRuleWorkbenchStore({ api, storage: memoryStorage() });
    await store.initialize();
    const source = JSON.stringify(definition(ruleA.id, '导入后的规则'));

    const first = store.importJson(source);
    await Promise.resolve();
    expect(await store.importJson(source)).toBe(false);
    expect(api.importRule).toHaveBeenCalledTimes(1);
    imported.resolve({ ...draft(ruleA.id, '导入后的规则'), revision: 9 });

    expect(await first).toBe(true);
    expect(store.importLoading).toBe(false);
    expect(store.document?.name).toBe('导入后的规则');
  });

  it('keeps the target document read only while importing JSON', async () => {
    const { api } = fakeApi();
    const imported = deferred<RuleDraft>();
    api.importRule = vi.fn(async () => imported.promise);
    const store = createRuleWorkbenchStore({ api, storage: memoryStorage() });
    await store.initialize();
    const source = JSON.stringify(definition(ruleA.id, '导入后的规则'));

    const importing = store.importJson(source);
    await vi.waitFor(() => expect(api.importRule).toHaveBeenCalledOnce());

    expect(store.documentReadOnly).toBe(true);
    store.setDefaultAction('ignore');
    expect(store.document?.default_action).toBe('download');

    imported.resolve({ ...draft(ruleA.id, '导入后的规则'), revision: 9 });
    expect(await importing).toBe(true);
    expect(store.documentReadOnly).toBe(false);
  });

  it('waits for pending draft changes before importing JSON', async () => {
    const { api } = fakeApi();
    const saved = deferred<RuleDraft>();
    const imported = deferred<RuleDraft>();
    api.saveDraft = vi.fn(async () => saved.promise);
    api.importRule = vi.fn(async () => imported.promise);
    const store = createRuleWorkbenchStore({ api, storage: memoryStorage() });
    await store.initialize();
    store.renameRule(ruleA.id, '导入前的编辑');
    const source = JSON.stringify(definition(ruleA.id, '导入后的规则'));

    const importing = store.importJson(source);
    await Promise.resolve();

    expect(api.saveDraft).toHaveBeenCalledOnce();
    expect(api.importRule).not.toHaveBeenCalled();

    saved.resolve({ ...draft(ruleA.id, '导入前的编辑'), revision: 8 });
    await vi.waitFor(() => expect(api.importRule).toHaveBeenCalledOnce());
    imported.resolve({ ...draft(ruleA.id, '导入后的规则'), revision: 9 });

    expect(await importing).toBe(true);
    expect(api.importRule).toHaveBeenCalledWith(
      ruleA.id,
      expect.objectContaining({ expected_revision: 8 })
    );
  });

  it('allows only one catalog operation state at a time', async () => {
    const { api } = fakeApi();
    const imported = deferred<RuleDraft>();
    api.importRule = vi.fn(async () => imported.promise);
    const store = createRuleWorkbenchStore({ api, storage: memoryStorage() });
    await store.initialize();
    const source = JSON.stringify(definition(ruleA.id, '导入后的规则'));

    const importing = store.importJson(source);
    await Promise.resolve();
    expect(store.importLoading).toBe(true);
    expect(store.creatingRule).toBe(false);
    expect(await store.createRule('并发新建')).toBe(false);
    await store.publish();
    expect(api.createRule).not.toHaveBeenCalled();
    expect(api.publishRule).not.toHaveBeenCalled();

    imported.resolve({ ...draft(ruleA.id, '导入后的规则'), revision: 9 });
    expect(await importing).toBe(true);
    expect(store.importLoading).toBe(false);
  });

  it('deletes the selected rule through optimistic revision control', async () => {
    const { api } = fakeApi();
    const store = createRuleWorkbenchStore({ api, storage: memoryStorage() });
    await store.initialize();

    expect(await store.deleteSelectedRule()).toBe(true);
    expect(api.deleteRule).toHaveBeenCalledWith(ruleA.id, ruleA.revision);
    expect(store.selectedRuleId).toBe(ruleB.id);
  });

  it('keeps a committed deletion when the next rule cannot be loaded', async () => {
    const { api } = fakeApi();
    api.loadDraft = vi.fn(async (id) => {
      if (id === ruleB.id) throw new Error('draft unavailable');
      return draft(id, ruleA.name);
    });
    const store = createRuleWorkbenchStore({ api, storage: memoryStorage() });
    await store.initialize();

    expect(await store.deleteSelectedRule()).toBe(true);

    expect(api.deleteRule).toHaveBeenCalledWith(ruleA.id, ruleA.revision);
    expect(store.rules.map((rule) => rule.id)).toEqual([ruleB.id]);
    expect(store.selectedRuleId).toBeNull();
    expect(store.document).toBeNull();
    expect(store.loadError).toBe('规则已删除，但下一条规则暂时无法读取');
  });

  it('keeps the current rule when saving fails before a selection change', async () => {
    const { api } = fakeApi();
    api.saveDraft = vi.fn(async () => {
      throw new Error('save failed');
    });
    const store = createRuleWorkbenchStore({ api, storage: memoryStorage() });
    await store.initialize();
    store.renameRule(ruleA.id, '尚未保存');

    await store.selectRule(ruleB.id);

    expect(store.selectedRuleId).toBe(ruleA.id);
    expect(store.selectedRule?.name).toBe('尚未保存');
    expect(store.loadError).toBe('当前规则保存失败，请重试');
    expect(api.loadDraft).toHaveBeenCalledTimes(1);
  });

  it('keeps the current rule when a selected rule draft cannot be loaded', async () => {
    const { api } = fakeApi();
    api.loadDraft = vi.fn(async (id) => {
      if (id === ruleB.id) throw new Error('draft unavailable');
      return draft(id, ruleA.name);
    });
    const store = createRuleWorkbenchStore({ api, storage: memoryStorage() });
    await store.initialize();

    await expect(store.selectRule(ruleB.id)).resolves.toBeUndefined();

    expect(store.selectedRuleId).toBe(ruleA.id);
    expect(store.selectedRule?.name).toBe(ruleA.name);
    expect(store.loadError).toBe('规则暂时无法读取');
  });

  it('keeps the current rule when a selected published rule cannot be exported', async () => {
    const { api } = fakeApi();
    api.loadDraft = vi.fn(async (id) => {
      if (id === ruleB.id) return null;
      return draft(id, ruleA.name);
    });
    api.exportRule = vi.fn(async (id) => {
      if (id === ruleB.id) throw new Error('published rule unavailable');
      return definition(id, ruleA.name);
    });
    const store = createRuleWorkbenchStore({ api, storage: memoryStorage() });
    await store.initialize();

    await expect(store.selectRule(ruleB.id)).resolves.toBeUndefined();

    expect(store.selectedRuleId).toBe(ruleA.id);
    expect(store.selectedRule?.name).toBe(ruleA.name);
    expect(store.loadError).toBe('规则暂时无法读取');
  });

  it('applies only the last rule selected when loads finish out of order', async () => {
    const { api } = fakeApi();
    api.listRules = vi.fn(async () =>
      [ruleA, ruleB, ruleC].map((rule) => structuredClone(rule))
    );
    const firstLoad = deferred<RuleDraft | null>();
    const lastLoad = deferred<RuleDraft | null>();
    api.loadDraft = vi.fn(async (id) => {
      if (id === ruleA.id) return draft(ruleA.id, ruleA.name);
      if (id === ruleB.id) return firstLoad.promise;
      return lastLoad.promise;
    });
    const store = createRuleWorkbenchStore({ api, storage: memoryStorage() });
    await store.initialize();

    const firstSelection = store.selectRule(ruleB.id);
    const lastSelection = store.selectRule(ruleC.id);
    lastLoad.resolve(draft(ruleC.id, ruleC.name));
    await lastSelection;
    firstLoad.resolve(draft(ruleB.id, ruleB.name));
    await firstSelection;

    expect(store.selectedRuleId).toBe(ruleC.id);
    expect(store.selectedRule?.name).toBe(ruleC.name);
  });

  it('keeps the current rule when it is clicked while another rule is loading', async () => {
    const { api } = fakeApi();
    const pending = deferred<RuleDraft | null>();
    api.loadDraft = vi.fn(async (id) => {
      if (id === ruleA.id) return draft(ruleA.id, ruleA.name);
      return pending.promise;
    });
    const store = createRuleWorkbenchStore({ api, storage: memoryStorage() });
    await store.initialize();

    const selectOther = store.selectRule(ruleB.id);
    await Promise.resolve();
    await store.selectRule(ruleA.id);
    pending.resolve(draft(ruleB.id, ruleB.name));
    await selectOther;

    expect(store.selectedRuleId).toBe(ruleA.id);
    expect(store.selectedRule?.name).toBe(ruleA.name);
  });

  it('waits for an active save before deleting the selected rule', async () => {
    const { api } = fakeApi();
    const save = deferred<RuleDraft>();
    api.saveDraft = vi.fn(async () => save.promise);
    const store = createRuleWorkbenchStore({ api, storage: memoryStorage() });
    await store.initialize();
    store.renameRule(ruleA.id, '保存中的规则');
    const saving = store.saveNow();

    const deletion = store.deleteSelectedRule();
    await Promise.resolve();
    expect(api.deleteRule).not.toHaveBeenCalled();
    save.resolve({ ...draft(ruleA.id, '保存中的规则'), revision: 8 });
    await saving;

    expect(await deletion).toBe(true);
    expect(api.deleteRule).toHaveBeenCalledWith(ruleA.id, ruleA.revision + 1);
  });

  it('saves a pending draft before deleting the selected rule', async () => {
    const { api } = fakeApi();
    const store = createRuleWorkbenchStore({ api, storage: memoryStorage() });
    await store.initialize();
    store.renameRule(ruleA.id, '删除前的编辑');

    expect(await store.deleteSelectedRule()).toBe(true);

    expect(api.saveDraft).toHaveBeenCalledOnce();
    expect(api.deleteRule).toHaveBeenCalledWith(ruleA.id, ruleA.revision + 1);
  });

  it('does not start another save while deleting the selected rule', async () => {
    vi.useFakeTimers();
    const { api } = fakeApi();
    const deleted = deferred<void>();
    api.deleteRule = vi.fn(async () => deleted.promise);
    const store = createRuleWorkbenchStore({
      api,
      storage: memoryStorage(),
      autosaveMs: 10
    });
    await store.initialize();

    const deletion = store.deleteSelectedRule();
    await vi.waitFor(() => expect(api.deleteRule).toHaveBeenCalledOnce());

    expect(store.documentReadOnly).toBe(true);
    store.renameRule(ruleA.id, '删除期间的编辑');
    await vi.advanceTimersByTimeAsync(10);
    expect(store.document?.name).toBe(ruleA.name);
    expect(api.saveDraft).not.toHaveBeenCalled();

    deleted.resolve(undefined);
    expect(await deletion).toBe(true);
    expect(store.documentReadOnly).toBe(false);
    vi.useRealTimers();
  });
});

function fakeApi(): { api: RuleWorkbenchApi } {
  const api: RuleWorkbenchApi = {
    listRules: vi.fn(async () => [
      structuredClone(ruleA),
      structuredClone(ruleB)
    ]),
    createRule: vi.fn(async () => structuredClone(ruleB)),
    copyRule: vi.fn(async () => structuredClone(ruleC)),
    reorderRules: vi.fn(async () => [
      structuredClone(ruleA),
      structuredClone(ruleB)
    ]),
    deleteRule: vi.fn(async () => undefined),
    loadDraft: vi.fn(async (ruleId) =>
      draft(ruleId, ruleId === ruleA.id ? ruleA.name : ruleB.name)
    ),
    saveDraft: vi.fn(async (ruleId, request) => ({
      id: '0198f64c-42a2-7374-bace-9f1c3b317fba',
      rule_id: ruleId,
      base_version: request.base_version,
      schema_version: 1,
      definition: structuredClone(request.definition),
      revision: 8
    })),
    publishRule: vi.fn(async (ruleId) => version(ruleId)),
    validateRule: vi.fn(async () => ({ valid: true })),
    exportRule: vi.fn(async (ruleId) =>
      definition(ruleId, ruleId === ruleA.id ? ruleA.name : ruleB.name)
    ),
    importRule: vi.fn(async (ruleId, request) => ({
      id: '0198f64c-42a2-7374-bace-9f1c3b317fbb',
      rule_id: ruleId,
      base_version: request.base_version,
      schema_version: 1,
      definition: structuredClone(request.definition),
      revision: 9
    })),
    previewRules: vi.fn<RuleWorkbenchApi['previewRules']>(
      async (_ruleId, request) => ({
        item: {
          pixiv_work_id: request.pixiv_work_id,
          title: '测试作品',
          artist_name: '测试作者',
          content_type: 'illustration',
          decision: 'download',
          matched_rule_id: ruleA.id,
          trace: { decision: 'download', matched_rule_id: ruleA.id, rules: [] }
        }
      })
    )
  };
  return { api };
}

function summary(id: string, name: string, sortOrder: number): RuleSummary {
  return {
    id,
    name,
    enabled: true,
    action: 'download',
    default_action: 'download',
    current_version_id: '0198f64c-42a2-7374-bace-9f1c3b317fb8',
    current_version: 2,
    lifecycle: 'modified',
    revision: 4,
    sort_order: sortOrder
  };
}

function definition(id: string, name: string) {
  return createRuleDefinition(id, name, 'download');
}

function draft(ruleId: string, name: string): RuleDraft {
  return {
    id: '0198f64c-42a2-7374-bace-9f1c3b317fb9',
    rule_id: ruleId,
    base_version: 2,
    schema_version: 1,
    definition: definition(ruleId, name),
    revision: 7
  };
}

function version(ruleId: string): RuleVersion {
  return {
    id: '0198f64c-42a2-7374-bace-9f1c3b317fb8',
    rule_id: ruleId,
    version: 3,
    base_version: 2,
    schema_version: 1,
    definition: definition(ruleId, ruleA.name),
    created_by: null
  };
}

function memoryStorage(initial: Record<string, string> = {}) {
  const values = { ...initial };
  return {
    getItem: (key: string) => values[key] ?? null,
    setItem: (key: string, value: string) => {
      values[key] = value;
    },
    removeItem: (key: string) => {
      delete values[key];
    }
  };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}
