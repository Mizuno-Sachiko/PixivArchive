<script lang="ts">
  import {
    descriptorForField,
    operatorLabels,
    operatorDescriptorForField,
    operatorsForField,
    ruleConditionHelp,
    ruleFields,
    type RuleCondition,
    type RuleField,
    type RuleOperator
  } from '$lib/api/rules';
  import type { RuleWorkbenchStore } from '$lib/stores/rule-workbench.svelte';
  import SelectField from '$lib/components/ui/SelectField.svelte';

  import ConditionValueEditor from './ConditionValueEditor.svelte';

  interface Props {
    store: RuleWorkbenchStore;
    ruleId: string;
    groupIndex: number;
    conditionIndex: number;
    condition: RuleCondition;
  }

  let { store, ruleId, groupIndex, conditionIndex, condition }: Props =
    $props();
  let labelPrefix = $derived(
    `条件组${groupIndex + 1}条件${conditionIndex + 1}`
  );
  let descriptor = $derived(descriptorForField(condition.field));
  let operators = $derived(operatorsForField(condition.field));
  let operatorDescriptor = $derived(
    operatorDescriptorForField(condition.field, condition.operator)
  );
</script>

<div class="condition-row">
  <span class="condition-number">{conditionIndex + 1}</span>
  <label>
    <span class="sr-only">{labelPrefix}字段</span>
    <SelectField
      ariaLabel={`${labelPrefix}字段`}
      value={condition.field}
      fullWidth
      options={ruleFields.map((field) => ({
        value: field.value,
        label: `${field.category} · ${field.label}`
      }))}
      onChange={(value) =>
        store.changeConditionField(
          ruleId,
          groupIndex,
          conditionIndex,
          value as RuleField
        )}
    />
  </label>

  <label>
    <span class="sr-only">{labelPrefix}运算符</span>
    <SelectField
      ariaLabel={`${labelPrefix}运算符`}
      value={condition.operator}
      fullWidth
      options={operators.map((operator) => ({
        value: operator,
        label: operatorLabels[operator]
      }))}
      onChange={(value) =>
        store.changeConditionOperator(
          ruleId,
          groupIndex,
          conditionIndex,
          value as RuleOperator
        )}
    />
  </label>

  <div class="value-cell">
    {#if !operatorDescriptor.requires_value}
      <span class="no-value" aria-hidden="true"></span>
    {:else if condition.value}
      <ConditionValueEditor
        {descriptor}
        value={condition.value}
        ariaLabel={`${labelPrefix}值`}
        onChange={(value) =>
          store.changeConditionValue(ruleId, groupIndex, conditionIndex, value)}
      />
    {:else}
      <span class="no-value" aria-hidden="true"></span>
    {/if}
  </div>

  <button
    class="remove"
    type="button"
    aria-label={`删除${labelPrefix}`}
    onclick={() => store.removeCondition(ruleId, groupIndex, conditionIndex)}
    >×</button
  >
</div>

<p class="condition-help">
  {ruleConditionHelp(descriptor, operatorDescriptor.requires_value)}
</p>

{#if descriptor.scope === 'page' || descriptor.type === 'tags' || descriptor.type === 'text'}
  <div class="condition-options">
    {#if descriptor.scope === 'page'}
      <label>
        <span>页面范围</span>
        <SelectField
          ariaLabel="页面范围"
          value={condition.page_quantifier ?? undefined}
          options={[
            { value: 'any_page', label: '任一页满足' },
            { value: 'all_pages', label: '全部页满足' }
          ]}
          onChange={(value) =>
            store.setConditionPageQuantifier(
              ruleId,
              groupIndex,
              conditionIndex,
              value as 'any_page' | 'all_pages'
            )}
        />
      </label>
    {/if}
    {#if descriptor.type === 'tags'}
      <label>
        <span>标签范围</span>
        <SelectField
          ariaLabel="标签范围"
          value={condition.tag_scope ?? undefined}
          options={[
            { value: 'original_and_translation', label: '原文和译名' },
            { value: 'original', label: '仅原文' }
          ]}
          onChange={(value) =>
            store.setConditionTagScope(
              ruleId,
              groupIndex,
              conditionIndex,
              value as 'original' | 'original_and_translation'
            )}
        />
      </label>
    {/if}
    {#if descriptor.type === 'tags' || descriptor.type === 'text'}
      <label class="checkbox">
        <input
          type="checkbox"
          checked={condition.case_sensitive ?? false}
          onchange={(event) =>
            store.setConditionCaseSensitive(
              ruleId,
              groupIndex,
              conditionIndex,
              event.currentTarget.checked
            )}
        />
        <span>区分大小写</span>
      </label>
    {/if}
  </div>
{/if}

<style>
  .condition-row {
    display: grid;
    grid-template-columns:
      24px minmax(145px, 1fr) minmax(128px, 0.86fr) minmax(150px, 1.05fr)
      28px;
    gap: 0.5rem;
    align-items: center;
  }

  .condition-number {
    color: var(--color-text-3);
    font: 0.62rem var(--font-mono);
    text-align: center;
  }

  .value-cell {
    display: flex;
    min-width: 0;
    gap: 0.35rem;
    align-items: center;
  }

  .no-value {
    display: block;
    width: 100%;
    height: 34px;
    border: 1px solid var(--color-border);
    border-radius: 7px;
    background: var(--color-surface-2);
    opacity: 0.58;
  }

  .remove {
    width: 28px;
    height: 28px;
    border-radius: 7px;
    background: transparent;
    color: var(--color-text-3);
    font-size: 1rem;
  }

  .remove:hover {
    background: var(--color-error-soft);
    color: var(--color-error);
  }

  .condition-options {
    display: flex;
    gap: 0.7rem;
    align-items: end;
    padding: 0.45rem 0 0 2rem;
  }

  .condition-help {
    padding: 0.25rem 0 0 2rem;
    margin: 0;
    color: var(--color-text-3);
    font-size: 0.62rem;
    line-height: 1.4;
  }

  .condition-options label:not(.checkbox) {
    display: grid;
    gap: 0.25rem;
    min-width: 126px;
  }

  .condition-options label > span {
    color: var(--color-text-3);
    font-size: 0.62rem;
  }

  .condition-row :global(.pa-select-trigger) {
    width: 100%;
    min-width: 0;
    height: 34px;
  }

  .condition-options :global(.pa-select-trigger) {
    height: 30px;
    background: var(--color-surface-2);
    font-size: 0.68rem;
  }

  .checkbox {
    display: flex;
    height: 30px;
    gap: 0.4rem;
    align-items: center;
  }

  .checkbox input {
    width: 14px;
    height: 14px;
  }

  @media (max-width: 1050px) {
    .condition-row {
      grid-template-columns:
        24px minmax(125px, 1fr) minmax(118px, 0.8fr) minmax(140px, 1fr)
        28px;
    }
  }

  @media (max-width: 760px) {
    .condition-row {
      grid-template-columns: 22px minmax(0, 1fr) 28px;
    }

    .condition-row > label,
    .value-cell {
      grid-column: 2;
    }

    .condition-number {
      grid-row: 1 / 4;
    }

    .remove {
      grid-row: 1;
      grid-column: 3;
    }

    .condition-options {
      flex-wrap: wrap;
      padding-left: 1.9rem;
    }
  }
</style>
