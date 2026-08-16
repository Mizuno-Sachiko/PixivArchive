<script lang="ts">
  import { onMount } from 'svelte';

  import { ruleWorkbenchApi, type RuleSummary } from '$lib/api/rules';
  import { subscriptionApi, type Subscription } from '$lib/api/subscriptions';
  import {
    AppEventRefreshCoordinator,
    currentAppEventVersion
  } from '$lib/app-event-refresh';
  import SubscriptionDrawer from '$lib/components/subscriptions/SubscriptionDrawer.svelte';
  import SubscriptionTable from '$lib/components/subscriptions/SubscriptionTable.svelte';
  import Field from '$lib/components/ui/Field.svelte';
  import PageHeader from '$lib/components/ui/PageHeader.svelte';
  import PanelHeader from '$lib/components/ui/PanelHeader.svelte';
  import RetryMessage from '$lib/components/ui/RetryMessage.svelte';
  import MetricStrip from '$lib/components/ui/MetricStrip.svelte';
  import SelectField from '$lib/components/ui/SelectField.svelte';
  import { LatestRequest } from '$lib/latest-request';
  import { subscriptionPresentation } from '$lib/subscription-status';

  const subscriptionResources = ['subscription', 'pixiv_account'] as const;
  let subscriptions = $state<Subscription[]>([]);
  let rules = $state<RuleSummary[]>([]);
  let selectedId = $state<string | null>(null);
  let kindFilter = $state('all');
  let stateFilter = $state('all');
  let subscriptionError = $state('');
  let ruleError = $state('');
  let subscriptionsLoading = $state(false);
  let rulesLoading = $state(false);
  const subscriptionRequests = new LatestRequest();
  const ruleRequests = new LatestRequest();
  const subscriptionRefresh = new AppEventRefreshCoordinator(loadSubscriptions);
  let filtered = $derived(
    subscriptions.filter((subscription) => {
      const kindMatches =
        kindFilter === 'all' || subscription.kind === kindFilter;
      const stateMatches =
        stateFilter === 'all' ||
        (stateFilter === 'enabled' && subscription.enabled) ||
        (stateFilter === 'disabled' && !subscription.enabled) ||
        (stateFilter === 'attention' &&
          subscriptionPresentation(subscription).requiresAttention);
      return kindMatches && stateMatches;
    })
  );
  let selected = $derived(
    subscriptions.find((subscription) => subscription.id === selectedId) ?? null
  );
  let enabledCount = $derived(
    subscriptions.filter((subscription) => subscription.enabled).length
  );
  let activeCount = $derived(
    subscriptions.filter(
      (subscription) =>
        subscription.pending_run || subscription.recent_state === 'running'
    ).length
  );
  let attentionCount = $derived(
    subscriptions.filter(
      (subscription) => subscriptionPresentation(subscription).requiresAttention
    ).length
  );

  $effect(() => {
    subscriptionRefresh.observe(currentAppEventVersion(subscriptionResources));
  });

  onMount(() => {
    subscriptionRefresh.start(currentAppEventVersion(subscriptionResources));
    void loadRules();
    return () => {
      subscriptionRefresh.dispose();
      subscriptionRequests.invalidate();
      ruleRequests.invalidate();
    };
  });

  async function loadSubscriptions(): Promise<boolean> {
    const request = subscriptionRequests.begin();
    subscriptionsLoading = true;
    try {
      const items = await subscriptionApi.list();
      if (!subscriptionRequests.isCurrent(request)) return false;
      subscriptions = items;
      subscriptionError = '';
      return true;
    } catch {
      if (subscriptionRequests.isCurrent(request)) {
        subscriptionError = '订阅列表暂时无法读取';
      }
      return false;
    } finally {
      if (subscriptionRequests.isCurrent(request)) subscriptionsLoading = false;
    }
  }

  async function loadRules(): Promise<void> {
    const request = ruleRequests.begin();
    rulesLoading = true;
    try {
      const availableRules = await ruleWorkbenchApi.listRules();
      if (!ruleRequests.isCurrent(request)) return;
      rules = availableRules;
      ruleError = '';
    } catch {
      if (ruleRequests.isCurrent(request)) {
        ruleError = '规则列表暂时无法读取';
      }
    } finally {
      if (ruleRequests.isCurrent(request)) rulesLoading = false;
    }
  }

  function replaceSubscription(updated: Subscription): void {
    subscriptions = subscriptions.map((subscription) =>
      subscription.id === updated.id ? updated : subscription
    );
  }

  function removeSubscription(id: string): void {
    subscriptions = subscriptions.filter(
      (subscription) => subscription.id !== id
    );
    if (selectedId === id) selectedId = null;
  }
</script>

<svelte:head>
  <title>订阅计划 · PixivArchive</title>
</svelte:head>

<section class="workspace-page">
  <PageHeader title="订阅计划" />

  <MetricStrip
    items={[
      { label: '全部订阅', value: String(subscriptions.length) },
      { label: '正在启用', value: String(enabledCount) },
      { label: '运行或待运行', value: String(activeCount) },
      { label: '需要处理', value: String(attentionCount) }
    ]}
  />

  {#if subscriptionError || ruleError}
    <div class="retry-list">
      {#if subscriptionError}
        <RetryMessage
          message={subscriptionError}
          busy={subscriptionsLoading}
          actionLabel="重新读取订阅列表"
          onRetry={() => subscriptionRefresh.retry()}
        />
      {/if}
      {#if ruleError}
        <RetryMessage
          message={ruleError}
          busy={rulesLoading}
          actionLabel="重新读取规则列表"
          onRetry={() => void loadRules()}
        />
      {/if}
    </div>
  {/if}

  <div class="workspace-layout">
    <section class="panel">
      <PanelHeader title="采集计划">
        {#snippet actions()}
          <span class="mono panel-subscription-count">{filtered.length}条</span>
        {/snippet}
      </PanelHeader>
      <div class="toolbar">
        <Field label="订阅类型" labelHidden class="subscription-filter">
          <SelectField
            bind:value={kindFilter}
            ariaLabel="订阅类型"
            fullWidth
            options={[
              { value: 'all', label: '全部类型' },
              { value: 'ranking', label: '排行榜' },
              { value: 'following', label: '关注作者' },
              { value: 'bookmarks', label: '收藏同步' }
            ]}
          />
        </Field>
        <Field label="订阅状态" labelHidden class="subscription-filter">
          <SelectField
            bind:value={stateFilter}
            ariaLabel="订阅状态"
            fullWidth
            options={[
              { value: 'all', label: '全部状态' },
              { value: 'enabled', label: '正在启用' },
              { value: 'disabled', label: '已经停用' },
              { value: 'attention', label: '需要处理' }
            ]}
          />
        </Field>
      </div>
      <SubscriptionTable
        items={filtered}
        {selectedId}
        onSelect={(id) => (selectedId = id)}
      />
    </section>

    <SubscriptionDrawer
      subscription={selected}
      {rules}
      onSaved={replaceSubscription}
      onDeleted={removeSubscription}
    />
  </div>
</section>

<style>
  :global(.subscription-filter) {
    min-width: 150px;
  }

  :global(.subscription-filter .pa-select-trigger) {
    min-height: 34px;
    padding-block: 0.35rem;
  }

  :global(.panel-subscription-count) {
    color: var(--color-text-3);
    font-size: 0.7rem;
  }

  .retry-list {
    display: grid;
    gap: 0.5rem;
  }
</style>
