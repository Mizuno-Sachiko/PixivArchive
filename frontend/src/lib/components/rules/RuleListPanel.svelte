<script lang="ts">
  import { flip } from 'svelte/animate';
  import { prefersReducedMotion } from 'svelte/motion';

  import type { RuleSummary } from '$lib/api/rules';
  import ConfirmDialog from '$lib/components/ui/ConfirmDialog.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import PanelHeader from '$lib/components/ui/PanelHeader.svelte';
  import type { RuleWorkbenchStore } from '$lib/stores/rule-workbench.svelte';

  import RuleCatalogRow from './RuleCatalogRow.svelte';
  import RuleCreatePanel from './RuleCreatePanel.svelte';
  import RuleImportPanel from './RuleImportPanel.svelte';

  interface Props {
    store: RuleWorkbenchStore;
  }

  let { store }: Props = $props();
  let createOpen = $state(false);
  let deleteTarget = $state<RuleSummary | null>(null);
  let deleteReturnFocus = $state<HTMLElement | null>(null);
  let importOpen = $state(false);
  let draggedRuleId = $state<string | null>(null);
  let previewOrder = $state<string[] | null>(null);
  let displayedRules = $derived.by(() => {
    if (!previewOrder) return store.visibleRules;
    const rulesById = new Map(store.rules.map((rule) => [rule.id, rule]));
    return previewOrder.flatMap((ruleId) => {
      const rule = rulesById.get(ruleId);
      return rule ? [rule] : [];
    });
  });

  function openCreatePanel(): void {
    store.createRuleError = '';
    createOpen = true;
  }

  async function selectRule(ruleId: string): Promise<boolean> {
    await store.selectRule(ruleId);
    return store.selectedRuleId === ruleId;
  }

  async function exportRule(ruleId: string): Promise<void> {
    if (!(await selectRule(ruleId))) return;
    const definition = await store.exportJson();
    const rule = store.selectedRuleSummary;
    if (!definition || !rule) return;
    const blob = new Blob([`${JSON.stringify(definition, null, 2)}\n`], {
      type: 'application/json'
    });
    const href = URL.createObjectURL(blob);
    const anchor = document.createElement('a');
    anchor.href = href;
    anchor.download = `${rule.name}.rule.json`;
    anchor.click();
    URL.revokeObjectURL(href);
  }

  async function openImportPanel(ruleId: string): Promise<void> {
    if (!(await selectRule(ruleId))) return;
    store.importError = '';
    store.importNotice = '';
    importOpen = true;
  }

  async function openDeleteDialog(
    ruleId: string,
    returnFocus: HTMLElement
  ): Promise<void> {
    if (!(await selectRule(ruleId))) return;
    deleteReturnFocus = returnFocus;
    deleteTarget = store.selectedRuleSummary;
  }

  async function deleteRule(): Promise<void> {
    if (await store.deleteSelectedRule()) deleteTarget = null;
  }

  function beginDrag(ruleId: string): void {
    draggedRuleId = ruleId;
    previewOrder = store.rules.map((rule) => rule.id);
  }

  function previewRule(targetRuleId: string, after: boolean): void {
    const sourceRuleId = draggedRuleId;
    if (!sourceRuleId || !previewOrder || sourceRuleId === targetRuleId) return;

    const nextOrder = previewOrder.filter((ruleId) => ruleId !== sourceRuleId);
    const targetIndex = nextOrder.indexOf(targetRuleId);
    if (targetIndex < 0) return;
    nextOrder.splice(targetIndex + (after ? 1 : 0), 0, sourceRuleId);
    if (nextOrder.every((ruleId, index) => ruleId === previewOrder?.[index])) {
      return;
    }
    previewOrder = nextOrder;
  }

  async function dropRule(): Promise<void> {
    const orderedRuleIds = previewOrder;
    draggedRuleId = null;
    previewOrder = null;
    if (!orderedRuleIds) return;
    await store.reorderRules(orderedRuleIds);
  }

  function endDrag(): void {
    draggedRuleId = null;
    previewOrder = null;
  }
</script>

<section class="rule-list" aria-label="规则列表">
  <PanelHeader title="规则" titleWrapped={false} class="rule-list-heading">
    {#snippet actions()}
      <button
        class="create-button"
        type="button"
        aria-label="新建规则"
        title="新建规则"
        disabled={store.catalogOperationActive}
        onclick={openCreatePanel}
      >
        <Icon name="plus" size={17} />
      </button>
    {/snippet}
  </PanelHeader>

  <div class="search-row">
    <label class="rule-search">
      <Icon name="search" size={15} />
      <input
        bind:value={store.ruleSearch}
        aria-label="搜索规则"
        placeholder="搜索规则"
        autocomplete="off"
      />
    </label>
    {#if store.catalogError}
      <p role="alert">{store.catalogError}</p>
    {/if}
  </div>

  <div class="rule-order">
    {#each displayedRules as rule, index (rule.id)}
      <div
        class="rule-row"
        animate:flip={{ duration: prefersReducedMotion.current ? 0 : 160 }}
      >
        <RuleCatalogRow
          {rule}
          {index}
          active={rule.id === store.selectedRuleId}
          busy={store.catalogOperationActive}
          canDrag={store.canReorderRules}
          onSelect={(ruleId) => store.selectRule(ruleId)}
          onRename={(ruleId, name) => store.updateRuleName(ruleId, name)}
          onEnabledChange={(ruleId, enabled) =>
            store.updateRuleEnabled(ruleId, enabled)}
          onCopy={(ruleId) => store.copyRule(ruleId)}
          onImport={openImportPanel}
          onExport={exportRule}
          onDelete={openDeleteDialog}
          onDragStart={beginDrag}
          onDragOver={previewRule}
          onDrop={dropRule}
          onDragEnd={endDrag}
        />
      </div>
    {:else}
      <p class="empty">
        {store.rules.length === 0 ? '还没有规则' : '没有符合条件的规则'}
      </p>
    {/each}
  </div>

  {#if createOpen}
    <RuleCreatePanel {store} onClose={() => (createOpen = false)} />
  {/if}

  {#if importOpen}
    <RuleImportPanel
      {store}
      initialSource={JSON.stringify(store.document, null, 2)}
      onClose={() => (importOpen = false)}
    />
  {/if}
</section>

{#if deleteTarget}
  <ConfirmDialog
    title="删除规则"
    description={`确定删除“${deleteTarget.name}”？使用该规则的订阅会恢复为默认下载全部。`}
    confirmLabel="删除"
    tone="danger"
    busy={store.deletingRule}
    returnFocus={deleteReturnFocus}
    onConfirm={() => void deleteRule()}
    onCancel={() => (deleteTarget = null)}
  />
{/if}

<style>
  .rule-list {
    position: relative;
    display: grid;
    min-width: 0;
    grid-template-rows: auto auto minmax(0, 1fr);
    overflow: hidden;
    border-right: 1px solid var(--color-border);
    background: var(--color-surface-1);
  }

  :global(.panel-heading.rule-list-heading) {
    display: flex;
    min-height: 68px;
    align-items: center;
    justify-content: space-between;
    padding: 0.9rem 1rem 0.75rem;
    border-bottom: 1px solid var(--color-border);
  }

  :global(.panel-heading.rule-list-heading h2) {
    margin: 0;
    font-size: 0.98rem;
  }

  .create-button {
    display: grid;
    width: 34px;
    height: 34px;
    place-items: center;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    background: var(--color-surface-2);
    color: var(--color-primary);
  }

  .create-button:hover:not(:disabled) {
    border-color: color-mix(
      in srgb,
      var(--color-primary) 45%,
      var(--color-border)
    );
    background: var(--color-primary-soft);
  }

  .search-row {
    display: grid;
    gap: 0.42rem;
    padding: 0.65rem 0.75rem;
    border-bottom: 1px solid var(--color-border);
  }

  .rule-search {
    display: grid;
    height: 34px;
    grid-template-columns: auto minmax(0, 1fr);
    gap: 0.45rem;
    align-items: center;
    padding: 0 0.58rem;
    border: 1px solid var(--color-border);
    border-radius: 7px;
    background: var(--color-surface-2);
    color: var(--color-text-3);
  }

  .rule-search:focus-within {
    border-color: var(--color-primary);
    box-shadow: 0 0 0 2px var(--color-primary-soft);
  }

  .rule-search input {
    min-width: 0;
    height: 100%;
    padding: 0;
    border: 0;
    outline: none;
    background: transparent;
    color: var(--color-text-1);
    font-size: 0.72rem;
  }

  .search-row p {
    margin: 0;
    color: var(--color-error);
    font-size: 0.68rem;
  }

  .rule-order {
    overflow: auto;
    padding: 0.35rem 0;
  }

  .rule-row {
    min-width: 0;
  }

  .empty {
    margin: 1rem;
    color: var(--color-text-3);
    font-size: 0.76rem;
  }

  button:disabled {
    cursor: not-allowed;
    opacity: 0.46;
  }
</style>
