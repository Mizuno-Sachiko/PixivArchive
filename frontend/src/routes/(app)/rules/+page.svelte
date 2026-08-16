<script lang="ts">
  import { onMount } from 'svelte';

  import {
    AppEventRefreshCoordinator,
    currentAppEventVersion
  } from '$lib/app-event-refresh';
  import RuleEditorPanel from '$lib/components/rules/RuleEditorPanel.svelte';
  import RuleListPanel from '$lib/components/rules/RuleListPanel.svelte';
  import RuleTracePanel from '$lib/components/rules/RuleTracePanel.svelte';
  import PageHeader from '$lib/components/ui/PageHeader.svelte';
  import RetryMessage from '$lib/components/ui/RetryMessage.svelte';
  import {
    ruleWorkbenchStore,
    type NarrowRuleView
  } from '$lib/stores/rule-workbench.svelte';

  let refreshCoordinator: AppEventRefreshCoordinator | null = null;

  $effect(() => {
    refreshCoordinator?.observe(currentAppEventVersion(['rule']));
  });

  onMount(() => {
    refreshCoordinator = new AppEventRefreshCoordinator(() =>
      ruleWorkbenchStore.refresh()
    );
    refreshCoordinator.start(currentAppEventVersion(['rule']));
    return () => {
      refreshCoordinator?.dispose();
      refreshCoordinator = null;
    };
  });

  function selectView(view: NarrowRuleView): void {
    ruleWorkbenchStore.setNarrowView(view);
  }
</script>

<svelte:head>
  <title>规则工作台 · PixivArchive</title>
</svelte:head>

<section class="rules-page">
  <PageHeader title="规则工作台" variant="page" />

  <nav class="view-switcher" aria-label="规则工作台视图">
    <button
      type="button"
      class:active={ruleWorkbenchStore.narrowView === 'list'}
      onclick={() => selectView('list')}>规则列表视图</button
    >
    <button
      type="button"
      class:active={ruleWorkbenchStore.narrowView === 'editor'}
      onclick={() => selectView('editor')}>编辑器视图</button
    >
    <button
      type="button"
      class:active={ruleWorkbenchStore.narrowView === 'trace'}
      onclick={() => selectView('trace')}>测试结果视图</button
    >
  </nav>

  {#if ruleWorkbenchStore.loadError}
    <RetryMessage
      message={ruleWorkbenchStore.loadError}
      busy={ruleWorkbenchStore.catalogOperationActive}
      onRetry={() => refreshCoordinator?.retry()}
    />
  {/if}

  <div class="workbench" data-view={ruleWorkbenchStore.narrowView}>
    <RuleListPanel store={ruleWorkbenchStore} />
    <RuleEditorPanel store={ruleWorkbenchStore} />
    <RuleTracePanel store={ruleWorkbenchStore} />
  </div>
</section>

<style>
  .rules-page {
    display: grid;
    gap: 0.8rem;
  }

  .workbench {
    display: grid;
    min-height: 420px;
    height: calc(100vh - 286px);
    grid-template-columns: minmax(220px, 0.72fr) minmax(510px, 1.8fr) minmax(
        280px,
        0.9fr
      );
    overflow: hidden;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    box-shadow: 0 12px 35px rgba(22, 42, 64, 0.08);
  }

  .view-switcher {
    display: none;
  }

  @media (max-width: 1180px) {
    .workbench {
      grid-template-columns: 210px minmax(500px, 1.6fr) 270px;
    }
  }

  @media (max-width: 760px) {
    .view-switcher {
      display: grid;
      grid-template-columns: repeat(3, 1fr);
      padding: 3px;
      border: 1px solid var(--color-border);
      border-radius: 9px;
      background: var(--color-surface-2);
    }

    .view-switcher button {
      height: 34px;
      border-radius: 6px;
      background: transparent;
      color: var(--color-text-3);
      font-size: 0.68rem;
      font-weight: 650;
    }

    .view-switcher button.active {
      background: var(--color-surface-1);
      box-shadow: 0 2px 8px rgba(10, 25, 40, 0.1);
      color: var(--color-primary);
    }

    .workbench {
      min-height: 520px;
      height: calc(100vh - 250px);
      grid-template-columns: minmax(0, 1fr);
      border-radius: 10px;
    }

    .workbench[data-view='list'] :global(.rule-editor),
    .workbench[data-view='list'] :global(.trace-panel),
    .workbench[data-view='editor'] :global(.rule-list),
    .workbench[data-view='editor'] :global(.trace-panel),
    .workbench[data-view='trace'] :global(.rule-list),
    .workbench[data-view='trace'] :global(.rule-editor) {
      display: none;
    }
  }
</style>
