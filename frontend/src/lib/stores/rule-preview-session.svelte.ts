import { ApiError } from '$lib/api/client';
import {
  type RuleDefinition,
  type RulePreviewItem,
  type RuleWorkbenchApi
} from '$lib/api/rules';
import { LatestRequest } from '$lib/latest-request';

import { cloneRuleDefinition } from './rule-document-session.svelte';

export class RulePreviewSession {
  item = $state<RulePreviewItem | null>(null);
  loading = $state(false);
  error = $state('');

  private readonly requests = new LatestRequest();

  constructor(private readonly api: RuleWorkbenchApi) {}

  async preview(
    ruleId: string,
    definition: RuleDefinition,
    pixivWorkId: number,
    isCurrent: () => boolean
  ): Promise<void> {
    const request = this.requests.begin();
    this.loading = true;
    this.error = '';
    try {
      const response = await this.api.previewRules(ruleId, {
        definition: cloneRuleDefinition(definition),
        pixiv_work_id: pixivWorkId
      });
      if (!this.requests.isCurrent(request) || !isCurrent()) return;
      this.item = response.item;
    } catch (error) {
      if (!this.requests.isCurrent(request) || !isCurrent()) return;
      this.item = null;
      this.error =
        error instanceof ApiError ? error.message : '作品判断暂时无法完成';
    } finally {
      if (this.requests.isCurrent(request)) this.loading = false;
    }
  }

  reset(): void {
    this.requests.invalidate();
    this.item = null;
    this.loading = false;
    this.error = '';
  }
}
