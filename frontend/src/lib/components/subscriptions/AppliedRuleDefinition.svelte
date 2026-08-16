<script lang="ts">
  import {
    descriptorForField,
    operatorLabels,
    ruleActionLabel,
    ruleWorkbenchApi,
    type ConditionValue,
    type RuleCondition,
    type RuleDocument,
    type RuleSummary
  } from '$lib/api/rules';
  import type { Subscription } from '$lib/api/subscriptions';

  interface Props {
    subscription: Subscription;
    rules: RuleSummary[];
  }

  let { subscription, rules }: Props = $props();
  let ruleDocument = $state<RuleDocument | null>(null);
  let ruleLoading = $state(false);
  let ruleError = $state('');
  let ruleRequest = 0;
  let rule = $derived(
    rules.find((item) => item.id === subscription.rule_id) ?? null
  );

  $effect(() => {
    const id = subscription.rule_id;
    const currentRule = rules.find((item) => item.id === id) ?? null;
    const request = ++ruleRequest;
    ruleDocument = null;
    ruleError = '';
    ruleLoading = Boolean(id && currentRule?.current_version_id);
    if (id && currentRule?.current_version_id) {
      void loadRuleDocument(id, request);
    }
  });

  async function loadRuleDocument(id: string, request: number): Promise<void> {
    try {
      const document = await ruleWorkbenchApi.exportRule(id);
      if (request === ruleRequest) ruleDocument = document;
    } catch {
      if (request === ruleRequest) ruleError = '当前规则内容暂时无法读取';
    } finally {
      if (request === ruleRequest) ruleLoading = false;
    }
  }

  function conditionLabel(condition: RuleCondition): string {
    const field = descriptorForField(condition.field).label;
    const operator = operatorLabels[condition.operator];
    const value = condition.value ? ` ${conditionValue(condition.value)}` : '';
    const page = condition.page_quantifier
      ? `（${condition.page_quantifier === 'all_pages' ? '所有页面' : '任一页面'}）`
      : '';
    return `${page}${field} ${operator}${value}`;
  }

  function conditionValue(value: ConditionValue): string {
    switch (value.type) {
      case 'number':
      case 'text':
      case 'date':
      case 'duration_hours':
      case 'duration_days':
        return String(value.value);
      case 'number_range':
        return `${value.value.min}～${value.value.max}`;
      case 'text_list':
        return value.value.join('、');
      case 'date_range':
        return `${value.value.start}～${value.value.end}`;
    }
  }
</script>

<section class="definition-section" aria-label="应用规则">
  <h3>应用规则</h3>
  {#if !subscription.rule_id}
    <div class="rule-default">
      <strong>未应用规则</strong>
      <span>发现的作品全部下载</span>
    </div>
  {:else if !rule}
    <p class="inline-message error">关联的规则不存在</p>
  {:else}
    <div class="rule-heading">
      <div>
        <span>规则</span>
        <strong>{rule.name}</strong>
      </div>
      <div>
        <span>未命中动作</span>
        <strong>{ruleActionLabel(rule.default_action)}</strong>
      </div>
    </div>
    {#if ruleLoading}
      <p class="inline-message">正在读取规则…</p>
    {:else if ruleError}
      <p class="inline-message error">{ruleError}</p>
    {:else if !rule.current_version_id}
      <div class="rule-default">
        <strong>规则尚未保存</strong>
        <span>当前运行会下载全部作品</span>
      </div>
    {:else if ruleDocument}
      <div class="rule-list">
        <article class:disabled={!ruleDocument.enabled}>
          <header>
            <strong>{ruleDocument.name}</strong>
            <em
              >{ruleDocument.enabled
                ? ruleActionLabel(ruleDocument.action)
                : '已停用'}</em
            >
          </header>
          <p>
            {ruleDocument.group_mode === 'all'
              ? '全部条件组满足'
              : '任一条件组满足'}
          </p>
          {#each ruleDocument.groups as group, groupIndex (groupIndex)}
            <div class="condition-group">
              <strong
                >条件组{groupIndex + 1} · {group.mode === 'all'
                  ? '全部条件满足'
                  : '任一条件满足'}</strong
              >
              <ul>
                {#each group.conditions as condition, conditionIndex (conditionIndex)}
                  <li>{conditionLabel(condition)}</li>
                {/each}
              </ul>
            </div>
          {/each}
        </article>
      </div>
    {/if}
  {/if}
</section>

<style>
  .definition-section {
    display: grid;
    gap: 0.65rem;
  }

  .definition-section h3 {
    margin: 0;
    font-size: 0.82rem;
  }

  .rule-heading > div,
  .rule-default,
  .rule-list article {
    min-width: 0;
    padding: 0.75rem;
    border-radius: var(--radius-sm);
    background: var(--color-surface-2);
  }

  .rule-heading span,
  .rule-default span,
  .rule-list article > p {
    color: var(--color-text-3);
    font-size: 0.68rem;
  }

  .rule-heading {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
    gap: 0.55rem;
  }

  .rule-heading > div,
  .rule-default {
    display: grid;
    gap: 0.25rem;
  }

  .rule-heading strong,
  .rule-default strong {
    font-size: 0.76rem;
    overflow-wrap: anywhere;
  }

  .rule-list {
    display: grid;
    gap: 0.55rem;
  }

  .rule-list article {
    display: grid;
    gap: 0.55rem;
  }

  .rule-list article.disabled {
    opacity: 0.58;
  }

  .rule-list header {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    align-items: center;
    gap: 0.5rem;
  }

  .rule-list header strong {
    font-size: 0.76rem;
    overflow-wrap: anywhere;
  }

  .rule-list header em {
    color: var(--color-primary);
    font-size: 0.68rem;
    font-style: normal;
    font-weight: 700;
  }

  .rule-list article > p {
    margin: 0;
  }

  .condition-group {
    display: grid;
    gap: 0.35rem;
    padding-top: 0.5rem;
    border-top: 1px solid var(--color-border);
  }

  .condition-group strong,
  .condition-group li {
    font-size: 0.68rem;
  }

  .condition-group ul {
    display: grid;
    gap: 0.3rem;
    margin: 0;
    padding-left: 1.1rem;
    color: var(--color-text-2);
  }
</style>
