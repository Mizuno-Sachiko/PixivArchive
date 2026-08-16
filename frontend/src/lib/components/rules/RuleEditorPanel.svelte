<script lang="ts">
  import { ruleActions, type GroupMode, type RuleAction } from '$lib/api/rules';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import type { RuleWorkbenchStore } from '$lib/stores/rule-workbench.svelte';
  import SelectField from '$lib/components/ui/SelectField.svelte';

  import ConditionGroup from './ConditionGroup.svelte';

  interface Props {
    store: RuleWorkbenchStore;
  }

  let { store }: Props = $props();
  let rule = $derived(store.selectedRule);

  function saveLabel(): string {
    return {
      idle: '尚未保存',
      dirty: '等待自动保存',
      saving: '正在保存草稿',
      saved: '草稿已保存',
      conflict: '草稿版本冲突',
      error: '草稿保存失败'
    }[store.saveState];
  }

  async function exportConflictCopy(): Promise<void> {
    const document = await store.exportJson();
    if (!document) return;
    const name = store.selectedRuleSummary?.name ?? 'rule';
    const blob = new Blob([JSON.stringify(document, null, 2)], {
      type: 'application/json'
    });
    const url = URL.createObjectURL(blob);
    const link = window.document.createElement('a');
    link.href = url;
    link.download = `${name}.unsaved.rules.json`;
    link.click();
    URL.revokeObjectURL(url);
  }
</script>

<section class="rule-editor" aria-label="规则编辑器">
  <header class="editor-heading">
    <h2>{rule?.name ?? '选择一条规则'}</h2>
    <div class="save-state" class:problem={store.saveState === 'conflict'}>
      <i></i>
      {saveLabel()}
    </div>
  </header>

  {#if rule}
    <fieldset
      class="editor-scroll"
      data-editor-scroll
      disabled={store.documentReadOnly}
    >
      <div class="rule-basics">
        <label>
          <span>规则命中动作</span>
          <SelectField
            value={rule.action}
            ariaLabel="规则命中动作"
            fullWidth
            options={ruleActions.map((action) => ({
              value: action.value,
              label: action.label
            }))}
            onChange={(value) =>
              store.setRuleAction(rule.id, value as RuleAction)}
          />
        </label>
        <label>
          <span>条件组之间</span>
          <SelectField
            value={rule.group_mode}
            ariaLabel="条件组之间"
            fullWidth
            options={[
              { value: 'all', label: '全部满足' },
              { value: 'any', label: '任一满足' }
            ]}
            onChange={(value) =>
              store.setRuleGroupMode(rule.id, value as GroupMode)}
          />
        </label>
      </div>

      <div class="logic-note">
        <strong>满足条件组时执行命中动作，否则执行默认动作</strong>
      </div>

      <div class="condition-groups">
        {#each rule.groups as group, groupIndex (`${rule.id}-${groupIndex}`)}
          <ConditionGroup
            {store}
            ruleId={rule.id}
            {group}
            {groupIndex}
            groupCount={rule.groups.length}
          />
        {/each}
      </div>

      <button
        class="add-group"
        type="button"
        onclick={() => store.addConditionGroup(rule.id)}
      >
        添加条件组
      </button>

      <section class="default-action">
        <strong>默认动作</strong>
        <SelectField
          ariaLabel="默认动作"
          value={store.document?.default_action}
          options={ruleActions.map((action) => ({
            value: action.value,
            label: action.label
          }))}
          onChange={(value) => store.setDefaultAction(value as RuleAction)}
        />
      </section>
    </fieldset>

    <footer class="publish-bar">
      {#if store.saveState === 'conflict'}
        <div class="conflict-actions" role="alert">
          <span>规则已在其他页面更新，当前修改尚未保存</span>
          <button type="button" onclick={() => void exportConflictCopy()}>
            导出当前修改
          </button>
          <button
            type="button"
            onclick={() => void store.reloadAfterConflict()}
          >
            载入最新版本
          </button>
        </div>
      {:else if store.publishError}
        <span class="publish-error" role="alert">{store.publishError}</span>
      {:else if store.publishNotice}
        <span class="publish-success">{store.publishNotice}</span>
      {:else}
        <span></span>
      {/if}
      <button
        type="button"
        disabled={store.catalogOperationActive}
        onclick={() => void store.publish()}
      >
        {store.publishLoading ? '正在保存' : '保存'}
      </button>
    </footer>
  {:else}
    <EmptyState message="未选择规则" variant="editor" />
  {/if}
</section>

<style>
  .rule-editor {
    display: grid;
    min-width: 0;
    grid-template-rows: auto minmax(0, 1fr) auto;
    overflow: hidden;
    border-right: 1px solid var(--color-border);
    background: var(--color-bg-subtle);
  }

  .editor-heading {
    display: flex;
    min-height: 68px;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    padding: 0.85rem 1.15rem;
    border-bottom: 1px solid var(--color-border);
    background: var(--color-surface-1);
  }

  h2 {
    max-width: 420px;
    margin: 0;
    overflow: hidden;
    font-size: 0.98rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .save-state {
    display: flex;
    gap: 0.42rem;
    align-items: center;
    color: var(--color-text-3);
    font-size: 0.68rem;
  }

  .save-state i {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--color-success);
  }

  .save-state.problem {
    color: var(--color-error);
  }

  .save-state.problem i {
    background: var(--color-error);
  }

  .publish-error {
    color: var(--color-error);
  }

  .publish-success {
    color: var(--color-success);
  }

  .editor-scroll {
    min-width: 0;
    margin: 0;
    overflow: auto;
    border: 0;
    padding: 1rem 1.1rem 2.2rem;
    scroll-behavior: auto;
  }

  button:disabled {
    cursor: not-allowed;
    opacity: 0.42;
  }

  .rule-basics {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 0.65rem;
    padding: 0.9rem;
    border: 1px solid var(--color-border);
    border-radius: 10px;
    background: var(--color-surface-1);
  }

  .rule-basics label {
    display: grid;
    gap: 0.34rem;
  }

  .rule-basics label > span {
    color: var(--color-text-3);
    font-size: 0.64rem;
    font-weight: 650;
  }

  .logic-note {
    display: flex;
    gap: 0.65rem;
    align-items: baseline;
    padding: 0.75rem 0.15rem 0.6rem;
  }

  .logic-note strong {
    color: var(--color-primary);
    font-size: 0.7rem;
  }

  .condition-groups {
    display: grid;
    gap: 0.7rem;
  }

  .rule-basics :global(.pa-select-trigger) {
    width: 100%;
  }

  .add-group {
    width: 100%;
    height: 38px;
    margin-top: 0.7rem;
    border: 1px dashed
      color-mix(in srgb, var(--color-primary) 45%, var(--color-border));
    border-radius: 9px;
    background: var(--color-primary-soft);
    color: var(--color-primary);
    font-size: 0.72rem;
    font-weight: 680;
  }

  .default-action {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    margin-top: 0.8rem;
    padding: 0.85rem 0.9rem;
    border-left: 3px solid var(--color-text-3);
    background: var(--color-surface-1);
  }

  .default-action strong {
    font-size: 0.76rem;
  }

  .default-action :global(.pa-select-trigger) {
    width: 150px;
    flex: 0 0 auto;
  }

  .publish-bar {
    display: flex;
    min-height: 58px;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    padding: 0.7rem 1rem;
    border-top: 1px solid var(--color-border);
    background: var(--color-glass-strong);
    backdrop-filter: blur(18px);
  }

  .publish-bar span {
    overflow: hidden;
    color: var(--color-text-3);
    font: 0.66rem var(--font-mono);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .conflict-actions {
    display: flex;
    min-width: 0;
    align-items: center;
    gap: 0.45rem;
  }

  .conflict-actions span {
    color: var(--color-error);
    font-family: inherit;
  }

  .conflict-actions button {
    height: 30px;
    flex: 0 0 auto;
    padding: 0 0.65rem;
    border: 1px solid
      color-mix(in srgb, var(--color-error) 35%, var(--color-border));
    background: var(--color-surface-1);
    color: var(--color-error);
  }

  .publish-bar > button {
    height: 36px;
    padding: 0 0.9rem;
    border-radius: 8px;
    background: var(--color-primary);
    color: #fff;
    font-size: 0.73rem;
    font-weight: 700;
  }

  @media (max-width: 760px) {
    .rule-editor {
      border-right: 0;
    }

    .editor-scroll {
      padding: 0.8rem 0.75rem 2rem;
    }

    .rule-basics {
      grid-template-columns: 1fr;
    }

    .default-action {
      align-items: stretch;
      flex-direction: column;
    }

    .default-action :global(.pa-select-trigger) {
      width: 100%;
    }

    .publish-bar {
      align-items: stretch;
      flex-direction: column;
    }

    .conflict-actions {
      align-items: stretch;
      flex-wrap: wrap;
    }
  }
</style>
