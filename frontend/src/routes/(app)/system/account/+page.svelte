<script lang="ts">
  import { onMount } from 'svelte';

  import { favoritesApi, type FavoritesState } from '$lib/api/favorites';
  import { systemApi, type PixivAccount } from '$lib/api/system';
  import {
    AppEventRefreshCoordinator,
    currentAppEventVersion
  } from '$lib/app-event-refresh';
  import ClearPixivCredentialAction from '$lib/components/account/ClearPixivCredentialAction.svelte';
  import SubscriptionSyncControls from '$lib/components/subscriptions/SubscriptionSyncControls.svelte';
  import Button from '$lib/components/ui/Button.svelte';
  import Field from '$lib/components/ui/Field.svelte';
  import MetricStrip from '$lib/components/ui/MetricStrip.svelte';
  import PageHeader from '$lib/components/ui/PageHeader.svelte';
  import RetryMessage from '$lib/components/ui/RetryMessage.svelte';
  import SwitchField from '$lib/components/ui/SwitchField.svelte';
  import SettingsCard from '$lib/components/settings/SettingsCard.svelte';
  import SettingsFeedback from '$lib/components/settings/SettingsFeedback.svelte';
  import { formatDateTime } from '$lib/format';
  import { accountStateLabel } from '$lib/labels';
  import {
    isPixivAccountConflict,
    pixivAccountActionFailureMessage,
    pixivAccountFailureMessage,
    pixivAccountValidationResult
  } from '$lib/pixiv-account-errors';
  import { isPixivAccountAvailable } from '$lib/pixiv-account-status';
  import { SerialActionQueue } from '$lib/serial-action-queue';
  import { pixivAccountStore } from '$lib/stores/pixiv-account.svelte';

  const favoritesResources = ['subscription', 'pixiv_account'] as const;
  let account = $derived(pixivAccountStore.current);
  let actionAccount = $derived(
    account && isPixivAccountAvailable(account.state) ? account : null
  );
  let displayAccount = $derived(
    account && isPixivAccountAvailable(account.state) ? account : null
  );
  let availableActionAccount = $derived(
    actionAccount && isPixivAccountAvailable(actionAccount.state)
      ? actionAccount
      : null
  );
  let cookie = $state('');
  let bookmarkWriteback = $state(false);
  let busy = $state(false);
  let validationMessage = $state('');
  let credentialMessage = $state('');
  let credentialError = $state('');
  let bookmarkMessage = $state('');
  let bookmarkError = $state('');
  let bookmarkFeedbackAccountId = $state<string | null>(null);
  let bookmarkIntent = $state<boolean | null>(null);
  let bookmarkIntentRevision = 0;
  let error = $state('');
  let favoritesState = $state<FavoritesState | null>(null);
  let intervalMinutes = $state('30');
  let favoritesRunBusy = $state(false);
  let favoritesIntent = $state<boolean | null>(null);
  let favoritesIntentRevision = 0;
  let favoritesLoading = $state(true);
  let favoritesMessage = $state('');
  let favoritesLoadError = $state('');
  let favoritesActionError = $state('');
  let favoritesProjectionVisible = $derived(
    Boolean(
      displayAccount?.account_id &&
      favoritesState?.subscription.account_id === displayAccount.account_id
    )
  );
  let favoritesActionReady = $derived(
    Boolean(availableActionAccount?.account_id && favoritesProjectionVisible)
  );
  let accountRequest = 0;
  let favoritesRequest = 0;
  const favoritesRefresh = new AppEventRefreshCoordinator(loadFavorites);
  const bookmarkQueue = new SerialActionQueue();
  const favoritesQueue = new SerialActionQueue();

  onMount(() => {
    if (account) {
      syncAccountControls(account);
      if (account.state === 'validating') void validateSaved();
    } else {
      void loadAccount();
    }
    favoritesRefresh.start(favoritesVersion());
    return () => {
      favoritesRefresh.dispose();
      accountRequest += 1;
      favoritesRequest += 1;
    };
  });

  $effect(() => {
    favoritesRefresh.observe(favoritesVersion());
  });

  $effect(() => {
    const current = account;
    bookmarkWriteback =
      current && isPixivAccountAvailable(current.state)
        ? (bookmarkIntent ?? current.bookmark_writeback_enabled)
        : false;
  });

  $effect(() => {
    const owner = bookmarkFeedbackAccountId;
    const current = account;
    if (owner && current?.account_id !== owner) {
      clearBookmarkFeedback();
    }
  });

  function favoritesVersion(): string {
    const current = pixivAccountStore.current;
    return `${current?.account_id ?? 'none'}:${current?.state ?? 'none'}:${currentAppEventVersion(favoritesResources)}`;
  }

  async function loadAccount(): Promise<void> {
    const request = ++accountRequest;
    try {
      const loaded = await pixivAccountStore.load();
      if (request !== accountRequest) return;
      if (!loaded) {
        error = pixivAccountStore.error || 'Pixiv账户状态暂时无法读取';
        return;
      }
      syncAccountControls(loaded);
      if (loaded.state === 'validating') {
        await validateSaved();
      }
    } catch {
      if (request === accountRequest) {
        error = 'Pixiv账户状态暂时无法读取';
      }
    }
  }

  function applyAccount(updated: PixivAccount): void {
    pixivAccountStore.replace(updated);
    syncAccountControls(updated);
  }

  function syncAccountControls(updated: PixivAccount): void {
    bookmarkWriteback = isPixivAccountAvailable(updated.state)
      ? updated.bookmark_writeback_enabled
      : false;
  }

  function clearBookmarkFeedback(): void {
    bookmarkMessage = '';
    bookmarkError = '';
    bookmarkFeedbackAccountId = null;
  }

  async function saveAndValidate(): Promise<void> {
    error = '';
    validationMessage = '';
    credentialMessage = '';
    credentialError = '';
    if (!cookie.trim()) {
      error = '请输入Pixiv Cookie';
      return;
    }
    const request = ++accountRequest;
    const previousAccountId = account?.account_id ?? null;
    busy = true;
    try {
      const updated = await systemApi.updateAccount({ cookie });
      if (request !== accountRequest) return;
      cookie = '';
      applyAccount(updated);
      applyValidationResult(updated);
      if (updated.account_id !== previousAccountId) {
        favoritesRefresh.observe(favoritesVersion());
      }
    } catch (failure) {
      if (request === accountRequest) {
        cookie = '';
        error = pixivAccountFailureMessage(failure);
      }
    } finally {
      if (request === accountRequest) busy = false;
    }
  }

  async function validateSaved(): Promise<void> {
    const accountId = actionAccount?.account_id;
    if (!accountId || actionAccount?.state === 'unconfigured') return;
    const request = ++accountRequest;
    busy = true;
    error = '';
    validationMessage = '';
    credentialMessage = '';
    credentialError = '';
    try {
      const updated = await systemApi.validateAccount(accountId);
      if (
        request !== accountRequest ||
        pixivAccountStore.currentForAction?.account_id !== accountId
      ) {
        return;
      }
      applyAccount(updated);
      applyValidationResult(updated);
    } catch (failure) {
      if (isPixivAccountConflict(failure)) void pixivAccountStore.load();
      if (request === accountRequest) {
        error = isPixivAccountConflict(failure)
          ? pixivAccountActionFailureMessage(failure, 'Pixiv账户验证请求失败')
          : pixivAccountFailureMessage(failure);
      }
    } finally {
      if (request === accountRequest) busy = false;
    }
  }

  function applyValidationResult(updated: PixivAccount): void {
    const result = pixivAccountValidationResult(updated.state);
    if (result.error) {
      error = result.message;
      validationMessage = '';
    } else {
      error = '';
      validationMessage = result.message;
    }
  }

  async function updateBookmarkWriteback(enabled: boolean): Promise<void> {
    if (!availableActionAccount?.account_id) return;
    const revision = ++bookmarkIntentRevision;
    bookmarkIntent = enabled;
    bookmarkWriteback = enabled;
    clearBookmarkFeedback();
    await bookmarkQueue.enqueue(() => saveBookmarkWriteback(enabled, revision));
    if (revision === bookmarkIntentRevision) {
      bookmarkIntent = null;
      bookmarkWriteback =
        account && isPixivAccountAvailable(account.state)
          ? account.bookmark_writeback_enabled
          : false;
    }
  }

  async function saveBookmarkWriteback(
    enabled: boolean,
    intentRevision: number
  ): Promise<void> {
    const current = account;
    if (!current?.account_id || current.revision === null) return;
    const accountId = current.account_id;
    try {
      const updated = await systemApi.setBookmarkWriteback(
        enabled,
        current.revision,
        accountId
      );
      if (pixivAccountStore.current?.account_id !== accountId) return;
      applyAccount(updated);
      if (intentRevision === bookmarkIntentRevision) {
        bookmarkMessage = enabled ? '收藏回写已开启' : '收藏回写已关闭';
        bookmarkFeedbackAccountId = accountId;
      }
    } catch (failure) {
      if (isPixivAccountConflict(failure)) {
        await pixivAccountStore.load();
      }
      if (
        pixivAccountStore.current?.account_id === accountId &&
        intentRevision === bookmarkIntentRevision
      ) {
        bookmarkError = pixivAccountActionFailureMessage(
          failure,
          '收藏回写设置保存失败'
        );
        bookmarkFeedbackAccountId = accountId;
      }
    }
  }

  async function loadFavorites(): Promise<boolean> {
    await favoritesQueue.waitForIdle();
    const request = ++favoritesRequest;
    const currentAccount = pixivAccountStore.current;
    const accountId =
      currentAccount && isPixivAccountAvailable(currentAccount.state)
        ? currentAccount.account_id
        : null;
    favoritesLoading = true;
    favoritesLoadError = '';
    if (!accountId) {
      favoritesState = null;
      favoritesIntent = null;
      favoritesMessage = '';
      favoritesActionError = '';
      favoritesLoading = false;
      return true;
    }
    if (favoritesState?.subscription.account_id !== accountId) {
      favoritesState = null;
      favoritesIntent = null;
      favoritesMessage = '';
      favoritesActionError = '';
    }
    try {
      const loaded = await favoritesApi.get();
      if (
        request !== favoritesRequest ||
        pixivAccountStore.current?.account_id !== accountId
      ) {
        return false;
      }
      favoritesState = loaded;
      intervalMinutes = String(
        loaded.subscription.schedule.interval_minutes ?? 30
      );
      return true;
    } catch {
      if (
        request === favoritesRequest &&
        pixivAccountStore.current?.account_id === accountId
      ) {
        favoritesLoadError = '收藏同步状态暂时无法读取';
      }
      return false;
    } finally {
      if (request === favoritesRequest) favoritesLoading = false;
    }
  }

  async function saveFavorites(enabled: boolean): Promise<void> {
    if (!availableActionAccount?.account_id || !favoritesState) {
      return;
    }
    const revision = ++favoritesIntentRevision;
    const interval = Number(intervalMinutes);
    favoritesIntent = enabled;
    favoritesMessage = '';
    favoritesActionError = '';
    await favoritesQueue.enqueue(() =>
      saveFavoritesRequest(enabled, interval, revision)
    );
    if (revision === favoritesIntentRevision) {
      favoritesIntent = null;
    }
  }

  async function saveFavoritesRequest(
    enabled: boolean,
    interval: number,
    intentRevision: number
  ): Promise<void> {
    const current = availableActionAccount;
    const state = favoritesState;
    if (
      !current?.account_id ||
      !state ||
      state.subscription.account_id !== current.account_id
    ) {
      return;
    }
    const accountId = current.account_id;
    try {
      const updated = await favoritesApi.update(
        enabled,
        interval,
        state.subscription.revision,
        accountId
      );
      if (pixivAccountStore.current?.account_id !== accountId) return;
      favoritesState = updated;
      if (intentRevision === favoritesIntentRevision) {
        favoritesMessage = enabled ? '收藏同步已开启' : '收藏同步已关闭';
      }
    } catch (failure) {
      if (isPixivAccountConflict(failure)) void pixivAccountStore.load();
      if (
        pixivAccountStore.current?.account_id === accountId &&
        intentRevision === favoritesIntentRevision
      ) {
        favoritesActionError = pixivAccountActionFailureMessage(
          failure,
          '收藏同步设置保存失败'
        );
      }
    }
  }

  async function runFavorites(): Promise<void> {
    const current = availableActionAccount;
    if (!current?.account_id || !favoritesActionReady || favoritesRunBusy) {
      return;
    }
    const accountId = current.account_id;
    favoritesRunBusy = true;
    favoritesMessage = '';
    favoritesActionError = '';
    try {
      await favoritesApi.run(accountId);
      if (pixivAccountStore.current?.account_id !== accountId) return;
      favoritesMessage = '完整收藏同步任务已加入队列';
    } catch (failure) {
      if (isPixivAccountConflict(failure)) void pixivAccountStore.load();
      if (pixivAccountStore.current?.account_id === accountId) {
        favoritesActionError = pixivAccountActionFailureMessage(
          failure,
          '收藏同步任务建立失败'
        );
      }
    } finally {
      favoritesRunBusy = false;
    }
  }
</script>

<svelte:head>
  <title>Pixiv账户 · PixivArchive</title>
</svelte:head>

<section class="workspace-page">
  <PageHeader title="Pixiv账户" />

  <MetricStrip
    class="account-metrics"
    items={[
      {
        label: '当前状态',
        value: account ? accountStateLabel(account.state) : '读取中',
        valueSize: 'standard'
      },
      {
        label: '用户ID',
        value: String(displayAccount?.pixiv_user_id ?? '—'),
        valueSize: 'standard'
      },
      {
        label: '显示名称',
        value: displayAccount?.display_name ?? '—',
        valueSize: 'standard'
      },
      {
        label: '上次验证',
        value: formatDateTime(displayAccount?.last_validated_at ?? null),
        valueSize: 'compact'
      }
    ]}
  />

  <div class="account-panels">
    <SettingsCard title="凭据">
      <Field label="Pixiv Cookie">
        <textarea
          bind:value={cookie}
          aria-label="Pixiv Cookie"
          autocomplete="off"
          spellcheck="false"
          placeholder="PHPSESSID或完整Cookie"></textarea>
      </Field>
      {#snippet actions()}
        <Button variant="primary" {busy} onclick={saveAndValidate}
          >保存并验证</Button
        >
        <Button
          disabled={busy ||
            !actionAccount?.account_id ||
            actionAccount.state === 'unconfigured'}
          onclick={validateSaved}>重新验证</Button
        >
        <ClearPixivCredentialAction
          variant="panel"
          renderFeedback={false}
          disabled={busy}
          onfeedback={({ message, error: feedbackError }) => {
            credentialMessage = message;
            credentialError = feedbackError;
          }}
        />
      {/snippet}
      {#snippet feedback()}
        <SettingsFeedback
          message={credentialMessage || validationMessage}
          error={credentialError || error}
        />
      {/snippet}
    </SettingsCard>

    <SettingsCard title="收藏回写">
      <div class="settings-inline-feedback">
        <SwitchField
          checked={bookmarkWriteback}
          label="同步修改Pixiv收藏"
          description="开启后可在作品详情同步修改Pixiv收藏。"
          disabled={!availableActionAccount?.account_id}
          onchange={(enabled) => void updateBookmarkWriteback(enabled)}
        />
        <SettingsFeedback message={bookmarkMessage} error={bookmarkError} />
      </div>
    </SettingsCard>

    <SettingsCard title="收藏同步">
      {#if favoritesLoadError}
        <RetryMessage
          message={favoritesLoadError}
          busy={favoritesLoading}
          actionLabel="重新读取收藏同步状态"
          onRetry={() => favoritesRefresh.retry()}
        />
      {/if}
      <SubscriptionSyncControls
        bind:intervalMinutes
        intervalAriaLabel="收藏同步间隔"
        disabled={!favoritesActionReady}
        runBusy={favoritesRunBusy}
        lastFullReconciledAt={favoritesProjectionVisible
          ? favoritesState?.last_full_reconciled_at
          : null}
        onIntervalChange={() => {
          if (favoritesState) {
            void saveFavorites(
              favoritesIntent ?? favoritesState.subscription.enabled
            );
          }
        }}
        onRunFull={() => void runFavorites()}
      >
        {#snippet primary()}
          <SwitchField
            checked={favoritesProjectionVisible
              ? (favoritesIntent ??
                favoritesState?.subscription.enabled ??
                false)
              : false}
            label="同步Pixiv收藏"
            ariaLabel="启用收藏同步"
            disabled={!favoritesActionReady}
            onchange={(enabled) => void saveFavorites(enabled)}
          />
        {/snippet}
        {#snippet feedback()}
          <SettingsFeedback
            message={favoritesMessage}
            error={favoritesActionError}
          />
        {/snippet}
      </SubscriptionSyncControls>
    </SettingsCard>
  </div>
</section>

<style>
  .account-panels {
    display: grid;
    gap: 18px;
  }

  .settings-inline-feedback {
    display: flex;
    min-width: 0;
    align-items: center;
    flex-wrap: wrap;
    gap: 0.4rem 0.75rem;
  }

  .settings-inline-feedback :global(.switch-field.described) {
    align-items: center;
  }

  .settings-inline-feedback :global(.switch-field.described input),
  .settings-inline-feedback :global(.settings-feedback),
  .settings-inline-feedback :global(.inline-message) {
    margin-top: 0;
  }
</style>
