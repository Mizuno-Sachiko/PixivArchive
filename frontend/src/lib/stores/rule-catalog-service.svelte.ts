import {
  type RuleDefinition,
  type RuleDraft,
  type RuleSummary,
  type RuleVersion,
  type RuleWorkbenchApi
} from '$lib/api/rules';
import { SvelteMap } from 'svelte/reactivity';

export type RuleOperation =
  | { kind: 'idle' }
  | { kind: 'creating' }
  | { kind: 'copying'; ruleId: string }
  | { kind: 'refreshing' }
  | { kind: 'updating'; ruleId: string }
  | { kind: 'deleting'; ruleId: string }
  | { kind: 'publishing'; ruleId: string }
  | { kind: 'importing'; ruleId: string }
  | { kind: 'reordering' };

export type RuleOperationResult<T> =
  { started: false } | { started: true; value: T };

export class RuleCatalogService {
  rules = $state<RuleSummary[]>([]);
  operation = $state<RuleOperation>({ kind: 'idle' });

  constructor(private readonly api: RuleWorkbenchApi) {}

  get idle(): boolean {
    return this.operation.kind === 'idle';
  }

  locksDocument(ruleId: string): boolean {
    if (this.operation.kind === 'idle') return false;
    if (this.operation.kind === 'creating') return true;
    if (this.operation.kind === 'refreshing') return true;
    if (this.operation.kind === 'reordering') return false;
    return this.operation.ruleId === ruleId;
  }

  async load(): Promise<RuleSummary[]> {
    this.rules = await this.api.listRules();
    return this.rules;
  }

  summary(ruleId: string | null): RuleSummary | null {
    return this.rules.find((rule) => rule.id === ruleId) ?? null;
  }

  async perform<T>(
    operation: Exclude<RuleOperation, { kind: 'idle' }>,
    task: () => Promise<T>
  ): Promise<RuleOperationResult<T>> {
    if (!this.idle) return { started: false };
    this.operation = operation;
    try {
      return { started: true, value: await task() };
    } finally {
      if (sameOperation(this.operation, operation)) {
        this.operation = { kind: 'idle' };
      }
    }
  }

  async create(name: string): Promise<RuleSummary> {
    const created = await this.api.createRule(name, 'download');
    this.rules = [...this.rules, created];
    return created;
  }

  async copy(ruleId: string, name: string): Promise<RuleSummary> {
    const copied = await this.api.copyRule(ruleId, { name });
    this.rules = [...this.rules, copied];
    return copied;
  }

  async reorder(orderedRuleIds: string[]): Promise<void> {
    const previous = [...this.rules];
    const byId = new SvelteMap(previous.map((rule) => [rule.id, rule]));
    const ordered = orderedRuleIds.flatMap((ruleId) => {
      const rule = byId.get(ruleId);
      return rule ? [rule] : [];
    });
    if (ordered.length !== previous.length) {
      throw new Error('Rule order must contain the complete catalog');
    }
    this.rules = ordered;
    try {
      this.rules = await this.api.reorderRules({
        ordered_rule_ids: orderedRuleIds
      });
    } catch (error) {
      try {
        await this.load();
      } catch {
        this.rules = previous;
      }
      throw error;
    }
  }

  async delete(rule: RuleSummary): Promise<RuleSummary | null> {
    await this.api.deleteRule(rule.id, rule.revision);
    const index = this.rules.findIndex((item) => item.id === rule.id);
    this.rules = this.rules.filter((item) => item.id !== rule.id);
    return this.rules[Math.min(index, this.rules.length - 1)] ?? null;
  }

  async publish(
    ruleId: string,
    baseVersion: number | null,
    draftRevision: number
  ): Promise<RuleVersion> {
    const published = await this.api.publishRule(ruleId, {
      base_version: baseVersion,
      expected_draft_revision: draftRevision
    });
    this.applyPublishedVersion(published);
    try {
      const refreshed = (await this.api.listRules()).find(
        (rule) => rule.id === ruleId
      );
      if (refreshed) this.mergeSummary(refreshed);
    } catch {
      // Publishing is already committed. The returned version contains every
      // field needed to keep the local lifecycle usable until the next reload.
    }
    return published;
  }

  async import(
    ruleId: string,
    definition: RuleDefinition,
    baseVersion: number | null,
    draftRevision: number | null
  ): Promise<RuleDraft> {
    const imported = await this.api.importRule(ruleId, {
      expected_revision: draftRevision,
      base_version: baseVersion,
      definition
    });
    this.applyDraft(imported);
    return imported;
  }

  applyDraft(draft: RuleDraft): void {
    const summary = this.summary(draft.rule_id);
    if (!summary) return;
    this.applyDefinition(summary, draft.definition);
    summary.revision += 1;
    summary.lifecycle = summary.current_version === null ? 'draft' : 'modified';
  }

  private applyPublishedVersion(published: RuleVersion): void {
    const summary = this.summary(published.rule_id);
    if (!summary) return;
    this.applyDefinition(summary, published.definition);
    summary.current_version_id = published.id;
    summary.current_version = published.version;
    summary.lifecycle = 'published';
    summary.revision += 1;
  }

  private applyDefinition(
    summary: RuleSummary,
    definition: RuleDefinition
  ): void {
    summary.name = definition.name;
    summary.enabled = definition.enabled;
    summary.action = definition.action;
    summary.default_action = definition.default_action;
  }

  private mergeSummary(refreshed: RuleSummary): void {
    const index = this.rules.findIndex((rule) => rule.id === refreshed.id);
    if (index < 0) return;
    if (this.rules[index].revision > refreshed.revision) return;
    this.rules[index] = refreshed;
  }
}

function sameOperation(left: RuleOperation, right: RuleOperation): boolean {
  if (left.kind !== right.kind) return false;
  if ('ruleId' in left && 'ruleId' in right) {
    return left.ruleId === right.ruleId;
  }
  return true;
}
