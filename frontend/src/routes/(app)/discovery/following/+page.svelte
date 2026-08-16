<script lang="ts">
  import { onMount } from 'svelte';

  import type { FollowingAuthor } from '$lib/api/following';
  import {
    AppEventRefreshCoordinator,
    currentAppEventVersion
  } from '$lib/app-event-refresh';
  import FollowedAuthorRow from '$lib/components/following/FollowedAuthorRow.svelte';
  import { FollowingSelectionSession } from '$lib/components/following/following-selection.svelte';
  import { FollowingStateSession } from '$lib/components/following/following-state-session.svelte';
  import SelectionActions from '$lib/components/ui/SelectionActions.svelte';
  import SubscriptionSyncControls from '$lib/components/subscriptions/SubscriptionSyncControls.svelte';
  import AlertBanner from '$lib/components/ui/AlertBanner.svelte';
  import Button from '$lib/components/ui/Button.svelte';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import PageHeader from '$lib/components/ui/PageHeader.svelte';
  import PanelHeader from '$lib/components/ui/PanelHeader.svelte';
  import RetryMessage from '$lib/components/ui/RetryMessage.svelte';
  import SettingsFeedback from '$lib/components/settings/SettingsFeedback.svelte';
  import {
    isPixivAccountConflict,
    pixivAccountActionFailureMessage
  } from '$lib/pixiv-account-errors';
  import { pixivAccountNotice } from '$lib/pixiv-account-status';
  import { pixivAccountStore } from '$lib/stores/pixiv-account.svelte';

  const followingResources = ['subscription', 'pixiv_account'] as const;
  const followingSession = new FollowingStateSession();
  const selection = new FollowingSelectionSession();
  let followingState = $derived(followingSession.state);
  let loading = $derived(followingSession.loading);
  let loadError = $derived(followingSession.loadError);
  let loadRetryable = $derived(followingSession.loadRetryable);
  let intervalMinutes = $derived(followingSession.intervalMinutes);
  let runBusy = $state(false);
  let fullRunBusy = $state(false);
  let refreshBusy = $state(false);
  let batchBusy = $state(false);
  let message = $state('');
  let actionError = $state('');
  let currentAccountId = $derived(
    pixivAccountStore.current?.account_id ?? null
  );
  let actionAccountId = $derived(
    pixivAccountStore.currentForAction?.account_id ?? null
  );
  let accountNotice = $derived(
    pixivAccountStore.current
      ? pixivAccountNotice(pixivAccountStore.current.state)
      : null
  );
  let stateMatchesCurrentAccount = $derived(
    Boolean(
      followingState &&
      actionAccountId &&
      followingState.subscription.account_id === actionAccountId
    )
  );
  let enabledCount = $derived(
    followingState?.authors.filter((author) => author.enabled).length ?? 0
  );
  let selectedCount = $derived(selection.count);

  const followingRefresh = new AppEventRefreshCoordinator(load);

  $effect(() => {
    if (followingSession.setAccount(currentAccountId)) {
      selection.exit();
      message = '';
      actionError = '';
      runBusy = false;
      fullRunBusy = false;
      refreshBusy = false;
      batchBusy = false;
    }
    followingRefresh.observe(followingVersion());
  });

  onMount(() => {
    followingRefresh.start(followingVersion());
    return () => {
      followingRefresh.dispose();
      followingSession.dispose();
    };
  });

  function followingVersion(): string {
    return `${currentAccountId ?? ''}:${currentAppEventVersion(followingResources)}`;
  }

  async function load(): Promise<boolean> {
    const loaded = await followingSession.load();
    refreshAccountOnMismatch();
    if (loaded && followingSession.state) {
      selection.retain(
        followingSession.state.authors.map((author) => author.pixiv_artist_id)
      );
    }
    return loaded;
  }

  function refreshAccountOnMismatch(): void {
    if (followingSession.accountMismatch) void pixivAccountStore.load();
  }

  function requireActionAccountId(): string | null {
    if (stateMatchesCurrentAccount) return actionAccountId;
    actionError = currentAccountId
      ? '当前Pixiv账户已经变化，正在重新读取'
      : '请先配置Pixiv账户';
    return null;
  }

  function recoverFromConflict(cause: unknown): void {
    if (!isPixivAccountConflict(cause)) return;
    void pixivAccountStore.load().finally(() => followingRefresh.retry());
  }

  async function setSubscriptionEnabled(enabled: boolean): Promise<void> {
    await configureSubscription(enabled, '关注订阅状态已更新');
  }

  async function setSubscriptionInterval(): Promise<void> {
    if (!followingState) return;
    await configureSubscription(
      followingState.subscription.enabled,
      '自动同步间隔已更新'
    );
  }

  async function configureSubscription(
    enabled: boolean,
    successMessage: string
  ): Promise<void> {
    const accountId = requireActionAccountId();
    if (!followingState || !accountId) return;
    const interval = Number(intervalMinutes);
    message = '';
    actionError = '';
    try {
      const applied = await followingSession.updateSubscription(
        enabled,
        interval
      );
      refreshAccountOnMismatch();
      if (applied && currentAccountId === accountId) message = successMessage;
    } catch (cause) {
      if (currentAccountId === accountId) {
        actionError = pixivAccountActionFailureMessage(
          cause,
          '关注订阅状态更新失败'
        );
      }
      recoverFromConflict(cause);
    }
  }

  async function setAuthorEnabled(
    author: FollowingAuthor,
    enabled: boolean
  ): Promise<void> {
    const accountId = requireActionAccountId();
    if (!followingState || !accountId) return;
    message = '';
    actionError = '';
    try {
      await followingSession.updateAuthor(author.pixiv_artist_id, enabled);
    } catch (cause) {
      if (currentAccountId === accountId) {
        actionError = pixivAccountActionFailureMessage(
          cause,
          `${author.display_name}的采集状态更新失败`
        );
      }
      recoverFromConflict(cause);
    }
  }

  async function setSelectedAuthorsEnabled(enabled: boolean): Promise<void> {
    const accountId = requireActionAccountId();
    const pixivArtistIds = [...selection.ids];
    if (!accountId || pixivArtistIds.length === 0 || batchBusy) return;
    batchBusy = true;
    message = '';
    actionError = '';
    try {
      const applied = await followingSession.updateAuthors(
        pixivArtistIds,
        enabled
      );
      refreshAccountOnMismatch();
      if (!applied || currentAccountId !== accountId) return;
      selection.retain(
        followingSession.state?.authors.map(
          (author) => author.pixiv_artist_id
        ) ?? []
      );
      message = enabled ? '所选作者已启用采集' : '所选作者已停止采集';
    } catch (cause) {
      actionError = pixivAccountActionFailureMessage(
        cause,
        enabled ? '批量启用采集失败' : '批量停止采集失败'
      );
      recoverFromConflict(cause);
    } finally {
      if (currentAccountId === accountId) batchBusy = false;
    }
  }

  async function runFollowing(backfill = false): Promise<void> {
    const accountId = requireActionAccountId();
    if (!accountId || runBusy || fullRunBusy) return;
    if (backfill) fullRunBusy = true;
    else runBusy = true;
    message = '';
    actionError = '';
    try {
      const applied = await followingSession.run(backfill);
      if (!applied || currentAccountId !== accountId) return;
      message = backfill
        ? '关注完整同步任务已加入队列'
        : '关注采集任务已加入队列';
    } catch (cause) {
      actionError = pixivAccountActionFailureMessage(
        cause,
        backfill ? '关注完整同步任务建立失败' : '关注采集任务建立失败'
      );
      recoverFromConflict(cause);
    } finally {
      if (currentAccountId === accountId) {
        if (backfill) fullRunBusy = false;
        else runBusy = false;
      }
    }
  }

  async function refreshFollowing(): Promise<void> {
    const accountId = requireActionAccountId();
    if (!accountId || refreshBusy) return;
    refreshBusy = true;
    message = '';
    actionError = '';
    try {
      const applied = await followingSession.refresh();
      refreshAccountOnMismatch();
      if (!applied || currentAccountId !== accountId) return;
      selection.retain(
        followingSession.state?.authors.map(
          (author) => author.pixiv_artist_id
        ) ?? []
      );
      message = '关注列表已刷新';
    } catch (cause) {
      actionError = pixivAccountActionFailureMessage(cause, '关注列表刷新失败');
      recoverFromConflict(cause);
    } finally {
      if (currentAccountId === accountId) refreshBusy = false;
    }
  }
</script>

<svelte:head>
  <title>关注订阅 · PixivArchive</title>
</svelte:head>

<section class="workspace-page">
  <PageHeader title="关注订阅" class="following-heading">
    {#snippet actions()}
      <SubscriptionSyncControls
        bind:intervalMinutes
        intervalAriaLabel="关注自动同步间隔"
        disabled={!stateMatchesCurrentAccount}
        runBusy={fullRunBusy || runBusy}
        lastFullReconciledAt={followingState?.last_full_reconciled_at}
        onIntervalChange={() => void setSubscriptionInterval()}
        onRunFull={() => void runFollowing(true)}
      >
        {#snippet primary()}
          <label class="subscription-switch">
            <input
              type="checkbox"
              role="switch"
              aria-label="启用关注订阅"
              checked={followingState?.subscription.enabled ?? false}
              disabled={!stateMatchesCurrentAccount}
              onchange={(event) =>
                void setSubscriptionEnabled(event.currentTarget.checked)}
            />
            <span>启用</span>
          </label>
          <Button
            variant="secondary"
            size="compact"
            disabled={!stateMatchesCurrentAccount || runBusy || fullRunBusy}
            onclick={() => void runFollowing()}>立即运行</Button
          >
        {/snippet}
        {#snippet feedback()}
          <SettingsFeedback {message} error={actionError} />
        {/snippet}
        {#snippet trailing()}
          <Button
            disabled={!stateMatchesCurrentAccount || refreshBusy}
            onclick={() => void refreshFollowing()}>刷新关注列表</Button
          >
        {/snippet}
      </SubscriptionSyncControls>
    {/snippet}
  </PageHeader>

  {#if accountNotice}
    <AlertBanner
      title={accountNotice.title}
      message={accountNotice.message}
      tone={accountNotice.tone}
    />
  {/if}

  {#if loadError}
    {#if loadRetryable}
      <RetryMessage
        message={loadError}
        busy={loading}
        actionLabel="重新读取关注列表"
        onRetry={() => followingRefresh.retry()}
      />
    {:else}
      <p class="inline-message error" role="alert">{loadError}</p>
    {/if}
  {/if}
  <section class="panel" aria-label="关注作者">
    <PanelHeader title="作者" titleWrapped={false}>
      {#snippet actions()}
        <div class="panel-actions">
          {#if selection.mode}
            <SelectionActions
              label={`${selectedCount}位已选择`}
              onSelectAll={() =>
                selection.selectAll(
                  followingState?.authors.map(
                    (author) => author.pixiv_artist_id
                  ) ?? []
                )}
              onInvert={() =>
                selection.invert(
                  followingState?.authors.map(
                    (author) => author.pixiv_artist_id
                  ) ?? []
                )}
              onClear={() => selection.clear()}
              onExit={() => selection.exit()}
            >
              {#snippet actions()}
                <Button
                  disabled={!stateMatchesCurrentAccount || selectedCount === 0}
                  onclick={() => void setSelectedAuthorsEnabled(true)}
                  >启用采集</Button
                >
                <Button
                  disabled={!stateMatchesCurrentAccount || selectedCount === 0}
                  onclick={() => void setSelectedAuthorsEnabled(false)}
                  >停止采集</Button
                >
              {/snippet}
            </SelectionActions>
          {:else}
            {#if followingState}
              <span class="author-count">
                共{followingState.authors.length}位关注作者 · {enabledCount}位启用采集
              </span>
            {/if}
            <Button
              disabled={!stateMatchesCurrentAccount ||
                !followingState?.authors.length}
              onclick={() => selection.enter()}>多选</Button
            >
          {/if}
        </div>
      {/snippet}
    </PanelHeader>

    {#if loading && !followingState}
      <EmptyState message="正在读取" loading />
    {:else if followingState?.authors.length}
      <ul class="author-list">
        {#each followingState.authors as author (author.pixiv_artist_id)}
          <FollowedAuthorRow
            {author}
            selectionMode={selection.mode}
            selected={selection.ids.has(author.pixiv_artist_id)}
            disabled={!stateMatchesCurrentAccount}
            onToggleSelection={() => selection.toggle(author.pixiv_artist_id)}
            onEnabledChange={(enabled) =>
              void setAuthorEnabled(author, enabled)}
          />
        {/each}
      </ul>
    {:else}
      <EmptyState message="没有关注作者" />
    {/if}
  </section>
</section>

<style>
  :global(.workspace-heading.following-heading) {
    align-items: end;
    flex-wrap: wrap;
  }

  :global(.workspace-heading.following-heading .subscription-sync-controls) {
    flex: 1 1 auto;
    min-width: 0;
    max-width: none;
  }

  .subscription-switch {
    display: inline-flex;
    align-items: center;
    gap: 0.45rem;
    color: var(--color-text-2);
    font-size: 0.76rem;
    font-weight: 680;
  }

  .subscription-switch {
    min-height: var(--control-height-md);
    padding: 0.45rem 0.7rem;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    background: var(--color-surface-2);
  }

  input[type='checkbox'] {
    width: 16px;
    height: 16px;
    margin: 0;
    accent-color: var(--color-primary);
  }

  .author-count {
    color: var(--color-text-3);
    font-size: 0.72rem;
  }

  .panel-actions {
    display: flex;
    min-width: 0;
    align-items: center;
    justify-content: flex-end;
    gap: 0.65rem;
  }

  .author-list {
    padding: 0;
    margin: 0;
    list-style: none;
  }

  @media (max-width: 980px) {
    :global(.workspace-heading.following-heading) {
      align-items: stretch;
      flex-direction: column;
    }
  }
</style>
