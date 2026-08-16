import { ConflictError } from '$lib/api/client';
import {
  createCondition,
  valueForOperator,
  type GroupMode,
  type PageQuantifier,
  type RuleAction,
  type RuleCondition,
  type RuleDefinition,
  type RuleDraft,
  type RuleField,
  type RuleOperator,
  type RuleSummary,
  type RuleWorkbenchApi,
  type TagScope
} from '$lib/api/rules';

export type RuleSaveState =
  'idle' | 'dirty' | 'saving' | 'saved' | 'conflict' | 'error';

export interface RuleSelectionStorage {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
  removeItem(key: string): void;
}

interface RuleDocumentSessionOptions {
  api: RuleWorkbenchApi;
  storage: RuleSelectionStorage;
  selectedRuleKey: string;
  autosaveMs: number;
  summaryFor(ruleId: string): RuleSummary | null;
  canEdit(ruleId: string): boolean;
  onSaved(draft: RuleDraft): void;
  onDirty(): void;
  onLoaded(): void;
}

export class RuleDocumentSession {
  selectedRuleId = $state<string | null>(null);
  document = $state<RuleDefinition | null>(null);
  baseVersion = $state<number | null>(null);
  draftRevision = $state<number | null>(null);
  saveState = $state<RuleSaveState>('idle');
  loadError = $state('');

  private autosaveTimer: ReturnType<typeof setTimeout> | null = null;
  private savePromise: Promise<void> | null = null;
  private editRevision = 0;
  private loadRevision = 0;
  private loadingRuleId: string | null = null;

  constructor(private readonly options: RuleDocumentSessionOptions) {}

  get revision(): number {
    return this.loadRevision;
  }

  storedRuleId(): string | null {
    return this.options.storage.getItem(this.options.selectedRuleKey);
  }

  async selectRule(ruleId: string): Promise<void> {
    if (ruleId === this.selectedRuleId) {
      if (this.loadingRuleId && this.loadingRuleId !== ruleId) {
        this.loadRevision += 1;
        this.loadingRuleId = null;
      }
      return;
    }
    const revision = ++this.loadRevision;
    await this.saveNow();
    if (revision !== this.loadRevision || !this.canLeaveCurrentRule()) return;
    await this.loadRule(ruleId, revision);
  }

  async loadRule(
    ruleId: string,
    revision = ++this.loadRevision
  ): Promise<void> {
    const summary = this.options.summaryFor(ruleId);
    if (!summary) return;
    this.cancelAutosave();
    if (revision === this.loadRevision) this.loadingRuleId = ruleId;
    try {
      const draft = await this.options.api.loadDraft(ruleId);
      let definition: RuleDefinition;
      let baseVersion: number | null;
      let draftRevision: number | null;
      if (draft) {
        definition = draft.definition;
        baseVersion = draft.base_version;
        draftRevision = draft.revision;
      } else {
        definition = await this.options.api.exportRule(ruleId);
        baseVersion = summary.current_version;
        draftRevision = null;
      }
      if (revision !== this.loadRevision) return;
      this.selectedRuleId = ruleId;
      this.options.storage.setItem(this.options.selectedRuleKey, ruleId);
      this.document = cloneRuleDefinition(definition);
      this.baseVersion = baseVersion;
      this.draftRevision = draftRevision;
      this.saveState = 'saved';
      this.options.onLoaded();
    } finally {
      if (revision === this.loadRevision && this.loadingRuleId === ruleId) {
        this.loadingRuleId = null;
      }
    }
  }

  async reload(): Promise<void> {
    if (!this.selectedRuleId) return;
    try {
      await this.loadRule(this.selectedRuleId);
      this.loadError = '';
    } catch {
      this.loadError = '规则暂时无法读取';
    }
  }

  beginTransition(): number {
    return ++this.loadRevision;
  }

  async prepareForDeletion(): Promise<void> {
    this.loadRevision += 1;
    await this.saveNow();
  }

  isCurrent(ruleId: string, revision: number): boolean {
    return this.selectedRuleId === ruleId && this.loadRevision === revision;
  }

  canLeaveCurrentRule(): boolean {
    if (this.saveState === 'conflict') {
      this.loadError = '当前规则存在版本冲突，请重新载入';
      return false;
    }
    if (this.saveState === 'error') {
      this.loadError = '当前规则保存失败，请重试';
      return false;
    }
    return true;
  }

  clearSelection(): void {
    this.loadRevision += 1;
    this.loadingRuleId = null;
    this.selectedRuleId = null;
    this.document = null;
    this.baseVersion = null;
    this.draftRevision = null;
    this.saveState = 'idle';
    this.options.storage.removeItem(this.options.selectedRuleKey);
    this.options.onLoaded();
  }

  replaceDocument(
    definition: RuleDefinition,
    baseVersion: number | null,
    draftRevision: number | null
  ): void {
    this.document = cloneRuleDefinition(definition);
    this.baseVersion = baseVersion;
    this.draftRevision = draftRevision;
    this.saveState = 'saved';
    this.options.onLoaded();
  }

  async exportCurrent(): Promise<RuleDefinition | null> {
    const ruleId = this.selectedRuleId;
    const revision = this.loadRevision;
    if (!ruleId) return null;
    await this.saveNow();
    if (!this.isCurrent(ruleId, revision)) return null;
    if (this.document) return cloneRuleDefinition(this.document);
    const exported = await this.options.api.exportRule(ruleId);
    return this.isCurrent(ruleId, revision) ? exported : null;
  }

  renameRule(ruleId: string, name: string): void {
    this.updateRule(ruleId, (rule) => (rule.name = name));
  }

  setRuleEnabled(ruleId: string, enabled: boolean): void {
    this.updateRule(ruleId, (rule) => (rule.enabled = enabled));
  }

  setRuleAction(ruleId: string, action: RuleAction): void {
    this.updateRule(ruleId, (rule) => (rule.action = action));
  }

  setRuleGroupMode(ruleId: string, mode: GroupMode): void {
    this.updateRule(ruleId, (rule) => (rule.group_mode = mode));
  }

  addConditionGroup(ruleId: string): void {
    this.updateRule(ruleId, (rule) => {
      rule.groups.push({
        mode: 'all',
        conditions: [createCondition('bookmark_count')]
      });
    });
  }

  removeConditionGroup(ruleId: string, groupIndex: number): void {
    this.updateRule(ruleId, (rule) => {
      if (rule.groups.length > 1) rule.groups.splice(groupIndex, 1);
    });
  }

  setConditionGroupMode(
    ruleId: string,
    groupIndex: number,
    mode: GroupMode
  ): void {
    this.updateConditionGroup(ruleId, groupIndex, (group) => {
      group.mode = mode;
    });
  }

  addCondition(ruleId: string, groupIndex: number): void {
    this.updateConditionGroup(ruleId, groupIndex, (group) => {
      group.conditions.push(createCondition('bookmark_count'));
    });
  }

  removeCondition(
    ruleId: string,
    groupIndex: number,
    conditionIndex: number
  ): void {
    this.updateConditionGroup(ruleId, groupIndex, (group) => {
      if (group.conditions.length > 1) {
        group.conditions.splice(conditionIndex, 1);
      }
    });
  }

  changeConditionField(
    ruleId: string,
    groupIndex: number,
    conditionIndex: number,
    field: RuleField
  ): void {
    this.updateCondition(ruleId, groupIndex, conditionIndex, (condition) => {
      const replacement = createCondition(field);
      const value = valueForOperator(
        field,
        replacement.operator,
        condition.value
      );
      if (value) replacement.value = value;
      else delete replacement.value;
      Object.assign(condition, replacement);
      for (const key of [
        'value',
        'case_sensitive',
        'tag_scope',
        'page_quantifier'
      ] as const) {
        if (!(key in replacement)) delete condition[key];
      }
    });
  }

  changeConditionOperator(
    ruleId: string,
    groupIndex: number,
    conditionIndex: number,
    operator: RuleOperator
  ): void {
    this.updateCondition(ruleId, groupIndex, conditionIndex, (condition) => {
      condition.operator = operator;
      const value = valueForOperator(
        condition.field,
        operator,
        condition.value
      );
      if (value) condition.value = value;
      else delete condition.value;
    });
  }

  changeConditionValue(
    ruleId: string,
    groupIndex: number,
    conditionIndex: number,
    value: RuleCondition['value']
  ): void {
    this.updateCondition(ruleId, groupIndex, conditionIndex, (condition) => {
      if (value) condition.value = value;
      else delete condition.value;
    });
  }

  setConditionPageQuantifier(
    ruleId: string,
    groupIndex: number,
    conditionIndex: number,
    value: PageQuantifier
  ): void {
    this.updateCondition(ruleId, groupIndex, conditionIndex, (condition) => {
      condition.page_quantifier = value;
    });
  }

  setConditionTagScope(
    ruleId: string,
    groupIndex: number,
    conditionIndex: number,
    value: TagScope
  ): void {
    this.updateCondition(ruleId, groupIndex, conditionIndex, (condition) => {
      condition.tag_scope = value;
    });
  }

  setConditionCaseSensitive(
    ruleId: string,
    groupIndex: number,
    conditionIndex: number,
    value: boolean
  ): void {
    this.updateCondition(ruleId, groupIndex, conditionIndex, (condition) => {
      condition.case_sensitive = value;
    });
  }

  setDefaultAction(action: RuleAction): void {
    if (!this.document || !this.options.canEdit(this.document.id)) return;
    this.document.default_action = action;
    this.markDirty();
  }

  async saveNow(): Promise<void> {
    this.cancelAutosave();
    if (this.savePromise) {
      await this.savePromise;
      if (this.saveState === 'dirty' || this.saveState === 'error') {
        await this.saveNow();
      }
      return;
    }
    while (
      this.selectedRuleId &&
      this.document &&
      (this.saveState === 'dirty' || this.saveState === 'error')
    ) {
      const savePromise = this.performSave();
      this.savePromise = savePromise;
      try {
        await savePromise;
      } finally {
        if (this.savePromise === savePromise) this.savePromise = null;
      }
      if (this.saveState !== 'dirty') break;
    }
  }

  private async performSave(): Promise<void> {
    if (
      !this.selectedRuleId ||
      !this.document ||
      (this.saveState !== 'dirty' && this.saveState !== 'error')
    ) {
      return;
    }
    const savingRevision = this.editRevision;
    this.saveState = 'saving';
    try {
      const saved = await this.options.api.saveDraft(this.selectedRuleId, {
        expected_revision: this.draftRevision,
        base_version: this.baseVersion,
        definition: cloneRuleDefinition(this.document)
      });
      this.baseVersion = saved.base_version;
      this.draftRevision = saved.revision;
      this.options.onSaved(saved);
      this.saveState = this.editRevision === savingRevision ? 'saved' : 'dirty';
    } catch (error) {
      this.saveState = error instanceof ConflictError ? 'conflict' : 'error';
    }
  }

  private updateRule(
    ruleId: string,
    update: (rule: RuleDefinition) => void
  ): void {
    if (
      !this.document ||
      this.document.id !== ruleId ||
      !this.options.canEdit(ruleId)
    ) {
      return;
    }
    update(this.document);
    this.markDirty();
  }

  private updateConditionGroup(
    ruleId: string,
    groupIndex: number,
    update: (group: RuleDefinition['groups'][number]) => void
  ): void {
    this.updateRule(ruleId, (rule) => {
      const group = rule.groups[groupIndex];
      if (group) update(group);
    });
  }

  private updateCondition(
    ruleId: string,
    groupIndex: number,
    conditionIndex: number,
    update: (condition: RuleCondition) => void
  ): void {
    this.updateConditionGroup(ruleId, groupIndex, (group) => {
      const condition = group.conditions[conditionIndex];
      if (condition) update(condition);
    });
  }

  private markDirty(): void {
    this.editRevision += 1;
    this.saveState = 'dirty';
    this.options.onDirty();
    this.scheduleAutosave();
  }

  private scheduleAutosave(): void {
    this.cancelAutosave();
    this.autosaveTimer = setTimeout(() => {
      this.autosaveTimer = null;
      void this.saveNow();
    }, this.options.autosaveMs);
  }

  private cancelAutosave(): void {
    if (this.autosaveTimer !== null) clearTimeout(this.autosaveTimer);
    this.autosaveTimer = null;
  }
}

export function cloneRuleDefinition(
  definition: RuleDefinition
): RuleDefinition {
  return $state.snapshot(definition);
}
