<script lang="ts">
  import type { RuleSummary } from '$lib/api/rules';
  import type { Subscription } from '$lib/api/subscriptions';
  import {
    rankingCombinationCount,
    rankingContentLabel,
    rankingModeLabel
  } from '$lib/rankings';
  import KeyValueGrid from '$lib/components/ui/KeyValueGrid.svelte';

  import AppliedRuleDefinition from './AppliedRuleDefinition.svelte';

  interface Props {
    subscription: Subscription;
    rules: RuleSummary[];
  }

  let { subscription, rules }: Props = $props();
  let modes = $derived(stringArray(subscription.params.modes));
  let contents = $derived(stringArray(subscription.params.contents));
  let maxRank = $derived(numberValue(subscription.params.max_rank, 20));
  let combinationCount = $derived(rankingCombinationCount(modes, contents));
  let sourceMode = $derived(stringValue(subscription.params.mode));
  let intervalMinutes = $derived(
    numberValue(subscription.schedule.interval_minutes, 0)
  );
  let lookbackPages = $derived(
    numberValue(subscription.schedule.lookback_pages, 0)
  );

  function stringArray(value: unknown): string[] {
    return Array.isArray(value)
      ? value.filter((item): item is string => typeof item === 'string')
      : [];
  }

  function stringValue(value: unknown): string {
    return typeof value === 'string' ? value : '';
  }

  function numberValue(value: unknown, defaultValue: number): number {
    return typeof value === 'number' ? value : defaultValue;
  }

  function accountLabel(): string {
    return `Pixiv ID ${subscription.account_pixiv_user_id}`;
  }

  function sourceModeLabel(): string {
    if (sourceMode === 'r18') return 'R-18作品';
    if (sourceMode === 'safe') return '全年龄作品';
    return sourceMode === 'all' ? '全部作品' : '未设置';
  }
</script>

<section class="definition-section" aria-label="采集对象">
  <h3>采集对象</h3>
  <KeyValueGrid>
    {#if subscription.kind === 'ranking'}
      <div>
        <dt>来源</dt>
        <dd>Pixiv排行榜</dd>
      </div>
      <div>
        <dt>Pixiv账户</dt>
        <dd>{accountLabel()}</dd>
      </div>
      <div class="wide">
        <dt>排行榜</dt>
        <dd class="chip-list">
          {#each modes as mode (mode)}
            <span>{rankingModeLabel(mode)}</span>
          {:else}
            <span>未设置</span>
          {/each}
        </dd>
      </div>
      <div class="wide">
        <dt>作品类型</dt>
        <dd class="chip-list">
          {#each contents as content (content)}
            <span>{rankingContentLabel(content)}</span>
          {:else}
            <span>未设置</span>
          {/each}
        </dd>
      </div>
      <div>
        <dt>每榜采集</dt>
        <dd>前{maxRank}名</dd>
      </div>
      <div>
        <dt>有效组合</dt>
        <dd>{combinationCount}个</dd>
      </div>
    {:else}
      <div class="wide">
        <dt>来源</dt>
        <dd>
          {subscription.kind === 'following' ? 'Pixiv关注动态' : 'Pixiv收藏'}
        </dd>
      </div>
      <div class="wide">
        <dt>Pixiv账户</dt>
        <dd>{accountLabel()}</dd>
      </div>
      <div>
        <dt>作品范围</dt>
        <dd>{sourceModeLabel()}</dd>
      </div>
      <div>
        <dt>运行间隔</dt>
        <dd>每{intervalMinutes}分钟</dd>
      </div>
      <div>
        <dt>补采范围</dt>
        <dd>最近{lookbackPages}页</dd>
      </div>
    {/if}
  </KeyValueGrid>
</section>

<AppliedRuleDefinition {subscription} {rules} />

<style>
  .definition-section {
    display: grid;
    gap: 0.65rem;
  }

  .definition-section h3 {
    margin: 0;
    font-size: 0.82rem;
  }

  .chip-list {
    display: flex;
    flex-wrap: wrap;
    gap: 0.35rem;
  }

  .chip-list span {
    padding: 0.26rem 0.48rem;
    border-radius: var(--radius-pill);
    background: var(--color-primary-soft);
    color: var(--color-primary);
    font-size: 0.68rem;
  }
</style>
