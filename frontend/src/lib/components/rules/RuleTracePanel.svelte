<script lang="ts">
  import {
    descriptorForField,
    operatorLabels,
    ruleActionLabel,
    type RuleTraceEntry
  } from '$lib/api/rules';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import PanelHeader from '$lib/components/ui/PanelHeader.svelte';
  import { parseSourceId } from '$lib/gallery-routes';
  import type { RuleWorkbenchStore } from '$lib/stores/rule-workbench.svelte';

  interface Props {
    store: RuleWorkbenchStore;
  }

  let { store }: Props = $props();
  let pixivWorkId = $state('');
  let inputError = $state('');
  let preview = $derived(store.previewItem);
  let matchedRule = $derived(
    preview?.trace.rules.find(
      (rule) => rule.rule_id === preview?.matched_rule_id
    )
  );

  function stateLabel(rule: RuleTraceEntry): string {
    return {
      skipped: '已停用',
      matched: '命中',
      not_matched: '未命中',
      stopped_before_evaluation: '前序规则已命中'
    }[rule.state];
  }

  function inspectWork(): void {
    inputError = '';
    const value = parseSourceId(pixivWorkId);
    if (value === null) {
      inputError = '请输入有效的数字作品PID';
      return;
    }
    void store.preview(value);
  }
</script>

<section class="trace-panel" aria-label="判断过程">
  <PanelHeader
    title="判断过程"
    titleWrapped={false}
    class="rule-trace-heading"
  />

  <div class="preview-controls">
    <label>
      <span>作品PID</span>
      <input
        bind:value={pixivWorkId}
        aria-label="作品PID"
        inputmode="numeric"
        autocomplete="off"
        placeholder="例如123456789"
        oninput={() => (inputError = '')}
        onkeydown={(event) => {
          if (event.key === 'Enter') inspectWork();
        }}
      />
    </label>
    <button type="button" disabled={store.previewLoading} onclick={inspectWork}>
      {store.previewLoading ? '判断中' : '检查作品'}
    </button>
    {#if inputError}<p class="input-error" role="alert">{inputError}</p>{/if}
  </div>

  <div class="trace-scroll">
    {#if store.previewError}
      <p class="trace-error" role="alert">{store.previewError}</p>
    {:else if preview}
      <section class="decision-card">
        <span>最终动作</span>
        <strong>{ruleActionLabel(preview.decision)}</strong>
        <small>Pixiv ID {preview.pixiv_work_id}</small>
      </section>

      <div class="matched-summary">
        {#if matchedRule}
          <strong>命中：{matchedRule.rule_name}</strong>
          <span>条件组满足，执行规则的命中动作。</span>
        {:else}
          <strong>使用默认动作</strong>
          <span>条件组未满足，执行规则的默认动作。</span>
        {/if}
      </div>

      <ol class="trace-list">
        {#each preview.trace.rules as traceRule (traceRule.rule_id)}
          <li class:matched={traceRule.state === 'matched'}>
            <header>
              <span>{String(traceRule.rule_index + 1).padStart(2, '0')}</span>
              <div>
                <strong>{traceRule.rule_name}</strong>
                <small>{stateLabel(traceRule)}</small>
              </div>
              <i class={traceRule.state}></i>
            </header>
            {#if traceRule.state !== 'stopped_before_evaluation'}
              <div class="group-traces">
                {#each traceRule.groups as group (group.group_index)}
                  <section>
                    <p>
                      条件组{group.group_index + 1} ·
                      {group.mode === 'all' ? '全部满足' : '任一满足'}
                    </p>
                    {#each group.conditions as condition (condition.condition_index)}
                      <div class="condition-trace">
                        <span
                          class:pass={condition.result === true}
                          class:fail={condition.result === false}
                        >
                          {condition.result === true
                            ? '✓'
                            : condition.result === false
                              ? '×'
                              : '—'}
                        </span>
                        <small>
                          {descriptorForField(condition.field).label}
                          {operatorLabels[condition.operator]}
                        </small>
                      </div>
                    {/each}
                  </section>
                {/each}
              </div>
            {/if}
          </li>
        {/each}
      </ol>
    {:else}
      <EmptyState variant="trace">
        <strong>尚未检查</strong>
      </EmptyState>
    {/if}
  </div>
</section>

<style>
  .trace-panel {
    display: grid;
    min-width: 0;
    grid-template-rows: auto auto minmax(0, 1fr);
    overflow: hidden;
    background: var(--color-surface-1);
  }

  :global(.panel-heading.rule-trace-heading) {
    display: flex;
    min-height: 68px;
    align-items: center;
    justify-content: space-between;
    padding: 0.85rem 1rem;
    border-bottom: 1px solid var(--color-border);
  }

  :global(.panel-heading.rule-trace-heading h2) {
    margin: 0;
    font-size: 0.98rem;
  }

  .preview-controls {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: 0.5rem;
    align-items: end;
    padding: 0.75rem 0.9rem;
    border-bottom: 1px solid var(--color-border);
    background: var(--color-surface-2);
  }

  .preview-controls label {
    display: grid;
    gap: 0.3rem;
  }

  .preview-controls label span {
    color: var(--color-text-3);
    font-size: 0.62rem;
    font-weight: 650;
  }

  .preview-controls input {
    width: 100%;
    height: 34px;
    padding: 0 0.65rem;
    border: 1px solid var(--color-border);
    border-radius: 7px;
    background: var(--color-surface-1);
    color: var(--color-text-1);
    font: 0.72rem var(--font-mono);
  }

  .input-error {
    grid-column: 1 / -1;
    margin: 0;
    color: var(--color-error);
    font-size: 0.66rem;
  }

  button {
    height: 34px;
    border-radius: 7px;
    font-size: 0.7rem;
  }

  button {
    padding: 0 0.68rem;
    background: var(--color-primary);
    color: #fff;
    font-weight: 680;
  }

  button:disabled {
    opacity: 0.6;
  }

  .trace-scroll {
    overflow: auto;
    padding: 0.85rem;
  }

  .decision-card {
    position: relative;
    display: grid;
    padding: 0.85rem 0.9rem;
    overflow: hidden;
    border-radius: 9px;
    background: var(--color-primary);
    color: #fff;
  }

  .decision-card::after {
    position: absolute;
    top: -28px;
    right: -15px;
    width: 86px;
    height: 86px;
    border: 18px solid rgba(255, 255, 255, 0.13);
    border-radius: 50%;
    content: '';
  }

  .decision-card span {
    font-size: 0.62rem;
    opacity: 0.78;
  }

  .decision-card strong {
    margin-top: 0.2rem;
    font-size: 1.08rem;
  }

  .decision-card small {
    margin-top: 0.55rem;
    font: 0.6rem var(--font-mono);
    opacity: 0.7;
  }

  .matched-summary {
    padding: 0.85rem 0.15rem;
    border-bottom: 1px solid var(--color-border);
  }

  .matched-summary strong,
  .matched-summary span {
    display: block;
  }

  .matched-summary strong {
    font-size: 0.78rem;
  }

  .matched-summary span {
    margin-top: 0.24rem;
    color: var(--color-text-3);
    font-size: 0.65rem;
  }

  .trace-list {
    display: grid;
    gap: 0.55rem;
    padding: 0;
    margin: 0.8rem 0 0;
    list-style: none;
  }

  .trace-list > li {
    border: 1px solid var(--color-border);
    border-radius: 8px;
    background: var(--color-surface-2);
  }

  .trace-list > li.matched {
    border-color: color-mix(
      in srgb,
      var(--color-primary) 45%,
      var(--color-border)
    );
    background: var(--color-primary-soft);
  }

  .trace-list > li > header {
    display: grid;
    min-height: 48px;
    grid-template-columns: 24px minmax(0, 1fr) 8px;
    gap: 0.55rem;
    align-items: center;
    padding: 0.55rem 0.65rem;
  }

  .trace-list header > span {
    color: var(--color-text-3);
    font: 0.6rem var(--font-mono);
  }

  .trace-list header strong,
  .trace-list header small {
    display: block;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .trace-list header strong {
    font-size: 0.72rem;
  }

  .trace-list header small {
    margin-top: 0.14rem;
    color: var(--color-text-3);
    font-size: 0.61rem;
  }

  .trace-list header i {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--color-text-3);
  }

  .trace-list header i.matched {
    background: var(--color-success);
  }

  .trace-list header i.not_matched {
    background: var(--color-error);
  }

  .group-traces {
    padding: 0 0.65rem 0.65rem 2.95rem;
  }

  .group-traces section {
    padding-top: 0.5rem;
    border-top: 1px solid var(--color-border);
  }

  .group-traces p {
    margin: 0 0 0.4rem;
    color: var(--color-text-3);
    font-size: 0.6rem;
  }

  .condition-trace {
    display: flex;
    gap: 0.4rem;
    align-items: center;
    margin-top: 0.3rem;
  }

  .condition-trace > span {
    color: var(--color-text-3);
    font: 0.72rem var(--font-mono);
  }

  .condition-trace > span.pass {
    color: var(--color-success);
  }

  .condition-trace > span.fail {
    color: var(--color-error);
  }

  .condition-trace small {
    color: var(--color-text-2);
    font-size: 0.62rem;
  }

  .trace-error {
    padding: 0.7rem;
    border-radius: 8px;
    margin: 0;
    background: var(--color-error-soft);
    color: var(--color-error);
    font-size: 0.72rem;
  }
</style>
