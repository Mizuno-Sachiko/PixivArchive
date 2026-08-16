import { ApiError, ConflictError } from '$lib/api/client';
import {
  ruleWorkbenchApi,
  type GroupMode,
  type PageQuantifier,
  type RuleAction,
  type RuleCondition,
  type RuleDefinition,
  type RuleField,
  type RuleOperator,
  type RulePreviewItem,
  type RuleSummary,
  type RuleWorkbenchApi,
  type TagScope
} from '$lib/api/rules';

import { RuleCatalogService } from './rule-catalog-service.svelte';
import {
  RuleDocumentSession,
  type RuleSaveState,
  type RuleSelectionStorage
} from './rule-document-session.svelte';
import { RulePreviewSession } from './rule-preview-session.svelte';

const SELECTED_RULE_KEY = 'pixivarchive.rules.selectedRule';

export type { RuleSaveState } from './rule-document-session.svelte';
export type NarrowRuleView = 'list' | 'editor' | 'trace';

interface RuleWorkbenchOptions {
  api?: RuleWorkbenchApi;
  storage?: RuleSelectionStorage;
  autosaveMs?: number;
}

export class RuleWorkbenchStore {
  ruleSearch = $state('');
  catalogError = $state('');
  importError = $state('');
  importNotice = $state('');
  publishNotice = $state('');
  publishError = $state('');
  createRuleError = $state('');
  narrowView = $state<NarrowRuleView>('list');
  initialized = $state(false);

  private readonly catalog: RuleCatalogService;
  private readonly documentSession: RuleDocumentSession;
  private readonly previewSession: RulePreviewSession;
  private initializationPromise: Promise<void> | null = null;

  constructor(options: RuleWorkbenchOptions = {}) {
    const api = options.api ?? ruleWorkbenchApi;
    const storage = options.storage ?? browserStorage();
    this.catalog = new RuleCatalogService(api);
    this.previewSession = new RulePreviewSession(api);
    this.documentSession = new RuleDocumentSession({
      api,
      storage,
      selectedRuleKey: SELECTED_RULE_KEY,
      autosaveMs: options.autosaveMs ?? 700,
      summaryFor: (ruleId) => this.catalog.summary(ruleId),
      canEdit: (ruleId) => !this.catalog.locksDocument(ruleId),
      onSaved: (draft) => this.catalog.applyDraft(draft),
      onDirty: () => {
        this.previewSession.reset();
        this.importNotice = '';
        this.publishNotice = '';
        this.publishError = '';
      },
      onLoaded: () => {
        this.previewSession.reset();
        this.publishNotice = '';
        this.publishError = '';
        this.importError = '';
        this.importNotice = '';
      }
    });
  }

  get rules(): RuleSummary[] {
    return this.catalog.rules;
  }

  get visibleRules(): RuleSummary[] {
    const query = this.ruleSearch.trim().toLocaleLowerCase();
    if (!query) return this.rules;
    return this.rules.filter((rule) =>
      rule.name.toLocaleLowerCase().includes(query)
    );
  }

  get canReorderRules(): boolean {
    return this.ruleSearch.trim() === '' && this.catalog.idle;
  }

  set rules(value: RuleSummary[]) {
    this.catalog.rules = value;
  }

  get selectedRuleId(): string | null {
    return this.documentSession.selectedRuleId;
  }

  get document(): RuleDefinition | null {
    return this.documentSession.document;
  }

  get baseVersion(): number | null {
    return this.documentSession.baseVersion;
  }

  get draftRevision(): number | null {
    return this.documentSession.draftRevision;
  }

  get saveState(): RuleSaveState {
    return this.documentSession.saveState;
  }

  get loadError(): string {
    return this.documentSession.loadError;
  }

  set loadError(value: string) {
    this.documentSession.loadError = value;
  }

  get importLoading(): boolean {
    return this.catalog.operation.kind === 'importing';
  }

  get publishLoading(): boolean {
    return this.catalog.operation.kind === 'publishing';
  }

  get creatingRule(): boolean {
    return this.catalog.operation.kind === 'creating';
  }

  get deletingRule(): boolean {
    return this.catalog.operation.kind === 'deleting';
  }

  get reorderingRules(): boolean {
    return this.catalog.operation.kind === 'reordering';
  }

  get catalogOperationActive(): boolean {
    return !this.catalog.idle;
  }

  get documentReadOnly(): boolean {
    return this.selectedRuleId !== null
      ? this.catalog.locksDocument(this.selectedRuleId)
      : false;
  }

  get previewItem(): RulePreviewItem | null {
    return this.previewSession.item;
  }

  get previewLoading(): boolean {
    return this.previewSession.loading;
  }

  get previewError(): string {
    return this.previewSession.error;
  }

  get selectedRuleSummary(): RuleSummary | null {
    return this.catalog.summary(this.selectedRuleId);
  }

  get selectedRule(): RuleDefinition | null {
    return this.document;
  }

  initialize(): Promise<void> {
    if (this.initialized) return Promise.resolve();
    if (this.initializationPromise) return this.initializationPromise;
    const initialization = this.loadInitialState().finally(() => {
      if (this.initializationPromise === initialization) {
        this.initializationPromise = null;
      }
    });
    this.initializationPromise = initialization;
    return initialization;
  }

  private async loadInitialState(): Promise<void> {
    try {
      const rules = await this.catalog.load();
      const storedRuleId = this.documentSession.storedRuleId();
      const selected = this.catalog.summary(storedRuleId) ?? rules[0] ?? null;
      if (selected) await this.documentSession.loadRule(selected.id);
      this.loadError = '';
      this.initialized = true;
    } catch {
      this.loadError = '规则暂时无法读取';
    }
  }

  async refresh(): Promise<boolean> {
    if (!this.initialized) {
      await this.initialize();
      return this.initialized;
    }
    if (!this.catalog.idle) return false;

    const previousRules = this.rules;
    try {
      const result = await this.catalog.perform(
        { kind: 'refreshing' },
        async () => {
          await this.saveNow();
          if (!this.documentSession.canLeaveCurrentRule()) return false;

          const selectedRuleId = this.selectedRuleId;
          const revision = this.documentSession.beginTransition();
          const rules = await this.catalog.load();
          const selected =
            this.catalog.summary(selectedRuleId) ?? rules[0] ?? null;
          if (selected) {
            await this.documentSession.loadRule(selected.id, revision);
          } else {
            this.documentSession.clearSelection();
          }
          this.loadError = '';
          return true;
        }
      );
      return result.started ? result.value : false;
    } catch {
      this.rules = previousRules;
      this.loadError = '规则暂时无法读取';
      return false;
    }
  }

  async selectRule(ruleId: string): Promise<void> {
    if (this.deletingRule) return;
    try {
      await this.documentSession.selectRule(ruleId);
      if (this.selectedRuleId === ruleId) this.loadError = '';
    } catch {
      this.loadError = '规则暂时无法读取';
    }
  }

  async createRule(name: string): Promise<boolean> {
    const normalized = name.trim();
    this.createRuleError = '';
    if (!normalized) {
      this.createRuleError = '请输入规则名称';
      return false;
    }
    try {
      const result = await this.catalog.perform(
        { kind: 'creating' },
        async () => {
          await this.saveNow();
          if (!this.documentSession.canLeaveCurrentRule()) {
            this.createRuleError =
              this.saveState === 'conflict'
                ? '当前规则存在版本冲突，请重新载入后再新建'
                : '当前规则保存失败，请保存后再新建';
            return false;
          }
          const revision = this.documentSession.beginTransition();
          const created = await this.catalog.create(normalized);
          try {
            await this.documentSession.loadRule(created.id, revision);
            this.loadError = '';
          } catch {
            this.loadError = '规则已创建，但内容暂时无法读取';
          }
          return true;
        }
      );
      return result.started ? result.value : false;
    } catch (error) {
      this.createRuleError =
        error instanceof ApiError ? error.message : '规则暂时无法创建';
      return false;
    }
  }

  async addRule(): Promise<void> {
    await this.createRule(`新规则${this.rules.length + 1}`);
  }

  async copyRule(ruleId: string): Promise<boolean> {
    const source = this.catalog.summary(ruleId);
    if (!source || !this.catalog.idle) return false;
    this.catalogError = '';
    await this.selectRule(ruleId);
    if (this.selectedRuleId !== ruleId) return false;
    try {
      const result = await this.catalog.perform(
        { kind: 'copying', ruleId },
        async () => {
          await this.saveNow();
          if (!this.documentSession.canLeaveCurrentRule()) {
            this.catalogError = this.loadError;
            return false;
          }
          const copied = await this.catalog.copy(ruleId, `${source.name} 副本`);
          const revision = this.documentSession.beginTransition();
          try {
            await this.documentSession.loadRule(copied.id, revision);
            this.loadError = '';
          } catch {
            this.loadError = '规则已复制，但内容暂时无法读取';
          }
          return true;
        }
      );
      return result.started ? result.value : false;
    } catch (error) {
      this.catalogError =
        error instanceof ApiError ? error.message : '规则暂时无法复制';
      return false;
    }
  }

  async updateRuleName(ruleId: string, name: string): Promise<boolean> {
    const normalized = name.trim();
    if (!normalized) {
      this.catalogError = '请输入规则名称';
      return false;
    }
    return this.updateCatalogRule(ruleId, () =>
      this.documentSession.renameRule(ruleId, normalized)
    );
  }

  async updateRuleEnabled(ruleId: string, enabled: boolean): Promise<boolean> {
    return this.updateCatalogRule(ruleId, () =>
      this.documentSession.setRuleEnabled(ruleId, enabled)
    );
  }

  async reorderRules(orderedRuleIds: string[]): Promise<boolean> {
    if (!this.canReorderRules) return false;
    this.catalogError = '';
    try {
      const result = await this.catalog.perform({ kind: 'reordering' }, () =>
        this.catalog.reorder(orderedRuleIds)
      );
      return result.started;
    } catch {
      this.catalogError = '规则顺序暂时无法保存';
      return false;
    }
  }

  private async updateCatalogRule(
    ruleId: string,
    update: () => void
  ): Promise<boolean> {
    if (!this.catalog.summary(ruleId) || !this.catalog.idle) return false;
    this.catalogError = '';
    await this.selectRule(ruleId);
    if (this.selectedRuleId !== ruleId || !this.catalog.idle) return false;
    update();
    try {
      const result = await this.catalog.perform(
        { kind: 'updating', ruleId },
        async () => {
          await this.saveNow();
          if (this.saveState === 'conflict' || this.saveState === 'error') {
            this.catalogError =
              this.saveState === 'conflict'
                ? '规则版本冲突，请重新载入'
                : '规则暂时无法保存';
            return false;
          }
          return true;
        }
      );
      return result.started ? result.value : false;
    } catch (error) {
      this.catalogError =
        error instanceof ApiError ? error.message : '规则暂时无法保存';
      return false;
    }
  }

  async deleteSelectedRule(): Promise<boolean> {
    const selected = this.selectedRuleSummary;
    if (!selected) return false;
    try {
      const result = await this.catalog.perform(
        { kind: 'deleting', ruleId: selected.id },
        async () => {
          await this.documentSession.prepareForDeletion();
          const next = await this.catalog.delete(selected);
          this.documentSession.clearSelection();
          if (next) {
            try {
              await this.documentSession.loadRule(next.id);
            } catch {
              this.loadError = '规则已删除，但下一条规则暂时无法读取';
              return true;
            }
          }
          return true;
        }
      );
      return result.started ? result.value : false;
    } catch (error) {
      this.loadError =
        error instanceof ConflictError
          ? '规则已被其他页面修改，请刷新后重试'
          : error instanceof ApiError
            ? error.message
            : '规则暂时无法删除';
      return false;
    }
  }

  renameRule(ruleId: string, name: string): void {
    this.documentSession.renameRule(ruleId, name);
  }

  setRuleEnabled(ruleId: string, enabled: boolean): void {
    this.documentSession.setRuleEnabled(ruleId, enabled);
  }

  setRuleAction(ruleId: string, action: RuleAction): void {
    this.documentSession.setRuleAction(ruleId, action);
  }

  setRuleGroupMode(ruleId: string, mode: GroupMode): void {
    this.documentSession.setRuleGroupMode(ruleId, mode);
  }

  addConditionGroup(ruleId: string): void {
    this.documentSession.addConditionGroup(ruleId);
  }

  removeConditionGroup(ruleId: string, groupIndex: number): void {
    this.documentSession.removeConditionGroup(ruleId, groupIndex);
  }

  setConditionGroupMode(
    ruleId: string,
    groupIndex: number,
    mode: GroupMode
  ): void {
    this.documentSession.setConditionGroupMode(ruleId, groupIndex, mode);
  }

  addCondition(ruleId: string, groupIndex: number): void {
    this.documentSession.addCondition(ruleId, groupIndex);
  }

  removeCondition(
    ruleId: string,
    groupIndex: number,
    conditionIndex: number
  ): void {
    this.documentSession.removeCondition(ruleId, groupIndex, conditionIndex);
  }

  changeConditionField(
    ruleId: string,
    groupIndex: number,
    conditionIndex: number,
    field: RuleField
  ): void {
    this.documentSession.changeConditionField(
      ruleId,
      groupIndex,
      conditionIndex,
      field
    );
  }

  changeConditionOperator(
    ruleId: string,
    groupIndex: number,
    conditionIndex: number,
    operator: RuleOperator
  ): void {
    this.documentSession.changeConditionOperator(
      ruleId,
      groupIndex,
      conditionIndex,
      operator
    );
  }

  changeConditionValue(
    ruleId: string,
    groupIndex: number,
    conditionIndex: number,
    value: RuleCondition['value']
  ): void {
    this.documentSession.changeConditionValue(
      ruleId,
      groupIndex,
      conditionIndex,
      value
    );
  }

  setConditionPageQuantifier(
    ruleId: string,
    groupIndex: number,
    conditionIndex: number,
    value: PageQuantifier
  ): void {
    this.documentSession.setConditionPageQuantifier(
      ruleId,
      groupIndex,
      conditionIndex,
      value
    );
  }

  setConditionTagScope(
    ruleId: string,
    groupIndex: number,
    conditionIndex: number,
    value: TagScope
  ): void {
    this.documentSession.setConditionTagScope(
      ruleId,
      groupIndex,
      conditionIndex,
      value
    );
  }

  setConditionCaseSensitive(
    ruleId: string,
    groupIndex: number,
    conditionIndex: number,
    value: boolean
  ): void {
    this.documentSession.setConditionCaseSensitive(
      ruleId,
      groupIndex,
      conditionIndex,
      value
    );
  }

  setDefaultAction(action: RuleAction): void {
    this.documentSession.setDefaultAction(action);
  }

  async saveNow(): Promise<void> {
    await this.documentSession.saveNow();
  }

  async publish(): Promise<void> {
    if (!this.catalog.idle) return;
    const ruleId = this.selectedRuleId;
    const revision = this.documentSession.revision;
    if (!ruleId) return;
    this.publishNotice = '';
    this.publishError = '';
    try {
      const result = await this.catalog.perform(
        { kind: 'publishing', ruleId },
        async () => {
          await this.saveNow();
          if (!this.documentSession.isCurrent(ruleId, revision)) return;
          if (this.saveState === 'conflict' || this.saveState === 'error') {
            this.publishError =
              this.saveState === 'conflict'
                ? '草稿版本冲突，请重新载入'
                : '草稿保存失败';
            return;
          }
          if (this.draftRevision === null) {
            this.publishNotice = '已保存';
            return;
          }
          const published = await this.catalog.publish(
            ruleId,
            this.baseVersion,
            this.draftRevision
          );
          if (!this.documentSession.isCurrent(ruleId, revision)) return;
          this.documentSession.replaceDocument(
            published.definition,
            published.version,
            null
          );
          this.publishNotice = '已保存';
        }
      );
      if (!result.started) return;
    } catch (error) {
      if (!this.documentSession.isCurrent(ruleId, revision)) return;
      this.publishError =
        error instanceof ConflictError
          ? '草稿版本冲突，请重新载入'
          : error instanceof ApiError
            ? error.message
            : '规则保存失败';
    }
  }

  async importJson(source: string): Promise<boolean> {
    if (!this.catalog.idle) return false;
    this.importError = '';
    this.importNotice = '';
    const ruleId = this.selectedRuleId;
    const revision = this.documentSession.revision;
    if (!ruleId) return false;
    let definition: RuleDefinition;
    try {
      definition = JSON.parse(source) as RuleDefinition;
    } catch {
      this.importError = 'JSON格式无法读取';
      return false;
    }
    if (definition.id !== ruleId) {
      this.importError = '导入规则ID与当前规则不一致';
      return false;
    }
    try {
      const result = await this.catalog.perform(
        { kind: 'importing', ruleId },
        async () => {
          await this.saveNow();
          if (!this.documentSession.isCurrent(ruleId, revision)) return false;
          if (this.saveState === 'conflict' || this.saveState === 'error') {
            this.importError =
              this.saveState === 'conflict'
                ? '草稿版本冲突，请重新载入'
                : '草稿保存失败';
            return false;
          }
          const imported = await this.catalog.import(
            ruleId,
            definition,
            this.baseVersion,
            this.draftRevision
          );
          if (!this.documentSession.isCurrent(ruleId, revision)) return true;
          this.documentSession.replaceDocument(
            imported.definition,
            imported.base_version,
            imported.revision
          );
          this.importNotice = '规则JSON已载入草稿';
          return true;
        }
      );
      return result.started ? result.value : false;
    } catch (error) {
      if (!this.documentSession.isCurrent(ruleId, revision)) return false;
      this.importError = importErrorMessage(error);
      return false;
    }
  }

  async exportJson(): Promise<RuleDefinition | null> {
    return this.documentSession.exportCurrent();
  }

  async reloadAfterConflict(): Promise<void> {
    await this.documentSession.reload();
  }

  async preview(pixivWorkId: number): Promise<void> {
    const ruleId = this.selectedRuleId;
    const revision = this.documentSession.revision;
    const document = this.document;
    if (!ruleId || !document) return;
    await this.previewSession.preview(ruleId, document, pixivWorkId, () =>
      this.documentSession.isCurrent(ruleId, revision)
    );
  }

  setNarrowView(view: NarrowRuleView): void {
    this.narrowView = view;
  }
}

export function createRuleWorkbenchStore(
  options: RuleWorkbenchOptions = {}
): RuleWorkbenchStore {
  return new RuleWorkbenchStore(options);
}

export const ruleWorkbenchStore = createRuleWorkbenchStore();

function browserStorage(): RuleSelectionStorage {
  if (typeof localStorage !== 'undefined') return localStorage;
  const values: Record<string, string> = {};
  return {
    getItem: (key) => values[key] ?? null,
    setItem: (key, value) => {
      values[key] = value;
    },
    removeItem: (key) => {
      delete values[key];
    }
  };
}

function importErrorMessage(error: unknown): string {
  if (error instanceof ApiError) {
    if (
      error.message.includes('unknown variant') ||
      error.message.toLowerCase().includes('regex')
    ) {
      return '规则包含不受支持的字段或运算符';
    }
    return error.message;
  }
  return '规则JSON验证失败';
}
