<script lang="ts">
  import { onMount } from 'svelte';

  import { subscriptionApi, type Subscription } from '$lib/api/subscriptions';
  import { ruleWorkbenchApi, type RuleSummary } from '$lib/api/rules';
  import {
    AppEventRefreshCoordinator,
    currentAppEventVersion
  } from '$lib/app-event-refresh';
  import Button from '$lib/components/ui/Button.svelte';
  import Field from '$lib/components/ui/Field.svelte';
  import KeyValueList from '$lib/components/ui/KeyValueList.svelte';
  import NumberField from '$lib/components/ui/NumberField.svelte';
  import PageHeader from '$lib/components/ui/PageHeader.svelte';
  import PanelHeader from '$lib/components/ui/PanelHeader.svelte';
  import RetryMessage from '$lib/components/ui/RetryMessage.svelte';
  import SelectField from '$lib/components/ui/SelectField.svelte';
  import TextField from '$lib/components/ui/TextField.svelte';
  import { LatestRequest } from '$lib/latest-request';
  import {
    rankingCombinationCount,
    rankingContents,
    rankingModes
  } from '$lib/rankings';
  import {
    MAX_SUBSCRIPTION_INTERVAL_MINUTES,
    MAX_SUBSCRIPTION_LOOKBACK_PAGES,
    MIN_SUBSCRIPTION_INTERVAL_MINUTES,
    subscriptionScheduleError
  } from '$lib/subscription-schedule';
  import {
    isPixivAccountConflict,
    pixivAccountActionFailureMessage
  } from '$lib/pixiv-account-errors';
  import { pixivAccountStore } from '$lib/stores/pixiv-account.svelte';

  const NO_RULE = '__none__';
  const rankingResources = ['rule'] as const;

  let account = $derived(pixivAccountStore.currentForAction);
  let rules = $state<RuleSummary[]>([]);
  let name = $state('');
  let selectedModes = $state<string[]>([]);
  let selectedContents = $state<string[]>([]);
  let maxRank = $state(20);
  let lookbackPages = $state(2);
  let intervalMinutes = $state(1440);
  let ruleId = $state(NO_RULE);
  let created = $state<Subscription | null>(null);
  let busy = $state(false);
  let loading = $state(true);
  let loadError = $state('');
  let actionError = $state('');
  const dependencyRequests = new LatestRequest();
  const dependencyRefresh = new AppEventRefreshCoordinator(loadDependencies);
  let validCombinationCount = $derived(
    rankingCombinationCount(selectedModes, selectedContents)
  );

  onMount(() => {
    dependencyRefresh.start(currentAppEventVersion(rankingResources));
    return () => {
      dependencyRefresh.dispose();
      dependencyRequests.invalidate();
    };
  });

  $effect(() => {
    dependencyRefresh.observe(currentAppEventVersion(rankingResources));
  });

  async function loadDependencies(): Promise<boolean> {
    const request = dependencyRequests.begin();
    loading = true;
    loadError = '';
    try {
      const loadedRules = await ruleWorkbenchApi.listRules();
      if (!dependencyRequests.isCurrent(request)) return false;
      rules = loadedRules;
      return true;
    } catch {
      if (dependencyRequests.isCurrent(request)) {
        loadError = '规则列表暂时无法读取';
      }
      return false;
    } finally {
      if (dependencyRequests.isCurrent(request)) loading = false;
    }
  }

  function toggleMode(value: string, checked: boolean): void {
    selectedModes = checked
      ? [...selectedModes, value]
      : selectedModes.filter((item) => item !== value);
  }

  function toggleContent(value: string, checked: boolean): void {
    selectedContents = checked
      ? [...selectedContents, value]
      : selectedContents.filter((item) => item !== value);
  }

  async function createSubscription(): Promise<void> {
    actionError = '';
    const accountId = account?.account_id;
    if (!accountId) {
      actionError = '请先配置能够采集排行榜的Pixiv账户';
      return;
    }
    if (
      !name.trim() ||
      selectedModes.length === 0 ||
      selectedContents.length === 0
    ) {
      actionError = '名称、榜单类型和作品类型都需要填写';
      return;
    }
    if (validCombinationCount === 0) {
      actionError = '所选榜单与作品类型没有有效组合';
      return;
    }
    const scheduleError = subscriptionScheduleError(
      intervalMinutes,
      lookbackPages
    );
    if (scheduleError) {
      actionError = scheduleError;
      return;
    }

    busy = true;
    try {
      const subscription = await subscriptionApi.create({
        kind: 'ranking',
        account_id: accountId,
        rule_id: ruleId === NO_RULE ? null : ruleId,
        name: name.trim(),
        interval_minutes: intervalMinutes,
        lookback_pages: lookbackPages,
        params: {
          modes: rankingModes
            .map((mode) => mode.value)
            .filter((mode) => selectedModes.includes(mode)),
          contents: rankingContents
            .map((content) => content.value)
            .filter((content) => selectedContents.includes(content)),
          max_rank: maxRank
        },
        next_run_at: null
      });
      if (pixivAccountStore.currentForAction?.account_id === accountId) {
        created = subscription;
      }
    } catch (cause) {
      actionError = pixivAccountActionFailureMessage(
        cause,
        '排行榜订阅建立失败'
      );
      if (isPixivAccountConflict(cause)) void pixivAccountStore.load();
    } finally {
      busy = false;
    }
  }
</script>

<svelte:head>
  <title>排行榜订阅 · PixivArchive</title>
</svelte:head>

<section class="workspace-page">
  <PageHeader title="排行榜订阅" />

  <div class="workspace-layout ranking-layout">
    <section class="panel">
      <PanelHeader title="采集范围" />
      <div class="form-body">
        {#if loadError}
          <RetryMessage
            message={loadError}
            busy={loading}
            actionLabel="重新读取规则列表"
            onRetry={() => dependencyRefresh.retry()}
          />
        {/if}
        <div class="field-grid">
          <TextField
            label="订阅名称"
            bind:value={name}
            placeholder="例如：每日综合与动图榜"
            wide
          />
          <Field label="规则">
            <SelectField
              bind:value={ruleId}
              ariaLabel="规则"
              fullWidth
              options={[
                { value: NO_RULE, label: '不应用下载规则' },
                ...rules.map((rule) => ({
                  value: rule.id,
                  label: rule.name
                }))
              ]}
            />
          </Field>
          <NumberField
            label="执行间隔（分钟）"
            min={MIN_SUBSCRIPTION_INTERVAL_MINUTES}
            max={MAX_SUBSCRIPTION_INTERVAL_MINUTES}
            step="1"
            bind:value={intervalMinutes}
          />
          <NumberField
            label="补采最近多少期"
            min="0"
            max={MAX_SUBSCRIPTION_LOOKBACK_PAGES}
            step="1"
            bind:value={lookbackPages}
          />
          <NumberField
            label="每个榜单采集前多少名"
            min="1"
            max="5000"
            bind:value={maxRank}
            wide
          />
        </div>

        <fieldset>
          <legend>排行榜类型</legend>
          <div class="choice-grid">
            {#each rankingModes as mode (mode.value)}
              <label class="check-field">
                <input
                  type="checkbox"
                  checked={selectedModes.includes(mode.value)}
                  onchange={(event) =>
                    toggleMode(
                      mode.value,
                      (event.currentTarget as HTMLInputElement).checked
                    )}
                />
                <span>{mode.label}</span>
              </label>
            {/each}
          </div>
        </fieldset>

        <fieldset>
          <legend>作品类型</legend>
          <div class="choice-grid content-types">
            {#each rankingContents as content (content.value)}
              <label class="check-field">
                <input
                  type="checkbox"
                  checked={selectedContents.includes(content.value)}
                  onchange={(event) =>
                    toggleContent(
                      content.value,
                      (event.currentTarget as HTMLInputElement).checked
                    )}
                />
                <span>{content.label}</span>
              </label>
            {/each}
          </div>
        </fieldset>

        <div class="button-row">
          <Button
            variant="primary"
            disabled={busy || !account}
            onclick={createSubscription}>创建排行榜订阅</Button
          >
        </div>
        {#if created}
          <p class="inline-message success">
            已建立排行榜订阅：{created.name}
          </p>
        {/if}
        {#if actionError}
          <p class="inline-message error" role="alert">{actionError}</p>
        {/if}
      </div>
    </section>

    <aside class="panel preview-panel">
      <PanelHeader title="订阅摘要" />
      <div class="detail-body">
        <KeyValueList variant="split">
          <div>
            <dt>Pixiv账户</dt>
            <dd>{account?.display_name ?? '尚未配置'}</dd>
          </div>
          <div>
            <dt>榜单数量</dt>
            <dd>{selectedModes.length}</dd>
          </div>
          <div>
            <dt>作品类型</dt>
            <dd>{selectedContents.length}</dd>
          </div>
          <div>
            <dt>单次最大候选</dt>
            <dd>{validCombinationCount * maxRank}</dd>
          </div>
        </KeyValueList>
      </div>
    </aside>
  </div>
</section>

<style>
  .ranking-layout {
    grid-template-columns: minmax(0, 1fr) minmax(300px, 0.34fr);
  }

  fieldset {
    padding: 0.9rem;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
  }

  legend {
    padding: 0 0.35rem;
    color: var(--color-text-2);
    font-size: 0.75rem;
    font-weight: 700;
  }

  .choice-grid {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 0.75rem;
  }

  .content-types {
    grid-template-columns: repeat(4, minmax(0, 1fr));
  }

  @media (max-width: 720px) {
    .choice-grid {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }
</style>
