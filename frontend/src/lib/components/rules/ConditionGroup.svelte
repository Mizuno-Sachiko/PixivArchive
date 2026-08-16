<script lang="ts">
  import type { GroupMode, RuleConditionGroup } from '$lib/api/rules';
  import SelectField from '$lib/components/ui/SelectField.svelte';
  import type { RuleWorkbenchStore } from '$lib/stores/rule-workbench.svelte';

  import ConditionEditor from './ConditionEditor.svelte';

  interface Props {
    store: RuleWorkbenchStore;
    ruleId: string;
    group: RuleConditionGroup;
    groupIndex: number;
    groupCount: number;
  }

  let { store, ruleId, group, groupIndex, groupCount }: Props = $props();
</script>

<section class="condition-group">
  <header>
    <div>
      <span class="group-index">组{groupIndex + 1}</span>
      <label>
        <span class="sr-only">条件组{groupIndex + 1}模式</span>
        <SelectField
          ariaLabel={`条件组${groupIndex + 1}模式`}
          value={group.mode}
          options={[
            { value: 'all', label: '组内全部满足' },
            { value: 'any', label: '组内任一满足' }
          ]}
          onChange={(value) =>
            store.setConditionGroupMode(ruleId, groupIndex, value as GroupMode)}
        />
      </label>
    </div>
    <button
      type="button"
      disabled={groupCount <= 1}
      onclick={() => store.removeConditionGroup(ruleId, groupIndex)}
    >
      删除条件组
    </button>
  </header>

  <div class="conditions">
    {#each group.conditions as condition, conditionIndex (`${ruleId}-${groupIndex}-${conditionIndex}`)}
      <div class="condition">
        <ConditionEditor
          {store}
          {ruleId}
          {groupIndex}
          {conditionIndex}
          {condition}
        />
      </div>
    {/each}
  </div>
  <button
    class="add-condition"
    type="button"
    onclick={() => store.addCondition(ruleId, groupIndex)}
  >
    ＋ 添加条件
  </button>
</section>

<style>
  .condition-group {
    overflow: hidden;
    border: 1px solid var(--color-border);
    border-radius: 10px;
    background: var(--color-surface-1);
  }

  .condition-group > header {
    display: flex;
    min-height: 45px;
    align-items: center;
    justify-content: space-between;
    padding: 0.5rem 0.7rem;
    border-bottom: 1px solid var(--color-border);
    background: var(--color-surface-2);
  }

  .condition-group > header > div {
    display: flex;
    gap: 0.55rem;
    align-items: center;
  }

  .condition-group header button {
    min-height: 30px;
    padding: 0 0.62rem;
    border: 1px solid var(--color-border);
    border-radius: 7px;
    background: var(--color-surface-1);
    color: var(--color-text-2);
    font-size: 0.68rem;
  }

  button:disabled {
    cursor: not-allowed;
    opacity: 0.42;
  }

  .group-index {
    color: var(--color-primary);
    font: 0.65rem var(--font-mono);
    font-weight: 700;
  }

  .condition-group header :global(.pa-select-trigger) {
    height: 29px;
    background: var(--color-surface-1);
    font-size: 0.68rem;
  }

  .conditions {
    padding: 0.35rem 0.75rem;
  }

  .condition {
    padding: 0.65rem 0;
    border-bottom: 1px solid var(--color-border);
  }

  .condition:last-child {
    border-bottom: 0;
  }

  .add-condition {
    margin: 0 0.75rem 0.7rem;
    padding: 0.42rem 0.58rem;
    border-radius: 6px;
    background: transparent;
    color: var(--color-primary);
    font-size: 0.68rem;
    font-weight: 650;
  }

  .add-condition:hover {
    background: var(--color-primary-soft);
  }
</style>
