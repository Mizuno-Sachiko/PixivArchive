<script lang="ts">
  import { tick } from 'svelte';

  import { systemApi } from '$lib/api/system';
  import ConfirmDialog from '$lib/components/ui/ConfirmDialog.svelte';
  import {
    isPixivAccountConflict,
    pixivAccountActionFailureMessage
  } from '$lib/pixiv-account-errors';
  import { pixivAccountStore } from '$lib/stores/pixiv-account.svelte';

  interface Props {
    variant?: 'menu' | 'panel';
    disabled?: boolean;
    renderFeedback?: boolean;
    onfeedback?: (feedback: { message: string; error: string }) => void;
  }

  interface FeedbackOwner {
    accountId: string | null;
    revision: number | null;
  }

  let {
    variant = 'panel',
    disabled = false,
    renderFeedback = true,
    onfeedback
  }: Props = $props();
  let account = $derived(pixivAccountStore.current);
  let canClear = $derived(
    Boolean(
      account?.account_id &&
      account.revision !== null &&
      account.state !== 'unconfigured'
    )
  );
  let confirming = $state(false);
  let busy = $state(false);
  let error = $state('');
  let success = $state('');
  let feedbackOwner = $state<FeedbackOwner | null>(null);
  let trigger = $state<HTMLButtonElement>();

  $effect(() => {
    if (
      feedbackOwner &&
      (account?.account_id !== feedbackOwner.accountId ||
        account.revision !== feedbackOwner.revision)
    ) {
      updateFeedback('', '');
    }
  });

  function updateFeedback(
    message: string,
    feedbackError: string,
    owner: FeedbackOwner | null = null
  ): void {
    success = message;
    error = feedbackError;
    feedbackOwner = owner;
    onfeedback?.({ message, error: feedbackError });
  }

  function beginClear(): void {
    if (!canClear || disabled || busy) return;
    updateFeedback('', '');
    confirming = true;
  }

  async function clearCredential(): Promise<void> {
    const current = pixivAccountStore.current;
    if (!current?.account_id || current.revision === null || busy) return;
    const accountId = current.account_id;
    const expectedRevision = current.revision;
    busy = true;
    error = '';
    try {
      const updated = await systemApi.clearAccountCredential(
        accountId,
        expectedRevision
      );
      confirming = false;
      await tick();
      const latest = pixivAccountStore.current;
      if (
        latest?.account_id !== accountId ||
        (latest.revision !== expectedRevision &&
          latest.revision !== updated.revision)
      ) {
        return;
      }
      pixivAccountStore.replace(updated);
      updateFeedback('Pixiv Cookie已清除', '', {
        accountId: updated.account_id,
        revision: updated.revision
      });
    } catch (failure) {
      if (isPixivAccountConflict(failure)) void pixivAccountStore.load();
      updateFeedback(
        '',
        pixivAccountActionFailureMessage(failure, 'Pixiv Cookie清除失败'),
        { accountId, revision: expectedRevision }
      );
    } finally {
      busy = false;
    }
  }
</script>

{#if canClear}
  <button
    bind:this={trigger}
    class:menu-action={variant === 'menu'}
    class:danger-button={variant === 'panel'}
    type="button"
    disabled={disabled || busy}
    onclick={beginClear}
  >
    {busy ? '正在清除…' : '清除Pixiv Cookie'}
  </button>
{/if}
{#if renderFeedback && success}
  <span class="action-message success" role="status">{success}</span>
{/if}
{#if renderFeedback && error && !confirming}
  <span class="action-message error" role="alert">{error}</span>
{/if}

{#if confirming}
  <ConfirmDialog
    title="清除Pixiv Cookie？"
    description="加密保存的Cookie将被删除，账户资料、已归档作品、订阅和历史记录会保留。"
    confirmLabel="清除Cookie"
    tone="danger"
    {busy}
    {error}
    returnFocus={trigger}
    onConfirm={() => void clearCredential()}
    onCancel={() => {
      if (!busy) confirming = false;
    }}
  />
{/if}

<style>
  .menu-action {
    display: flex;
    width: 100%;
    min-height: 38px;
    align-items: center;
    padding: 0.45rem 0.55rem;
    border-radius: 8px;
    background: transparent;
    color: var(--color-error);
    font-size: 0.84rem;
    text-align: left;
  }

  .menu-action:hover {
    background: var(--color-error-soft);
  }

  .action-message {
    display: block;
    padding: 0.45rem 0.55rem;
    font-size: 0.72rem;
    line-height: 1.45;
  }

  .action-message.success {
    color: var(--color-success);
  }

  .action-message.error {
    color: var(--color-error);
  }
</style>
