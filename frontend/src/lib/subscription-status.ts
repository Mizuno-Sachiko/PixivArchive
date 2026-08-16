import type { Subscription } from '$lib/api/subscriptions';
import { subscriptionStateLabel } from '$lib/labels';
import {
  pixivAccountNotice,
  type PixivAccountNotice
} from '$lib/pixiv-account-status';

export type SubscriptionStatusTone =
  'neutral' | 'success' | 'warning' | 'error' | 'primary';

export interface SubscriptionPresentation {
  label: string;
  tone: SubscriptionStatusTone;
  requiresAttention: boolean;
  accountTitle: string | null;
  accountMessage: string | null;
  blocksImmediateRun: boolean;
}

export function subscriptionPresentation(
  subscription: Subscription
): SubscriptionPresentation {
  const accountNotice = pixivAccountNotice(subscription.account_state);
  if (!subscription.enabled) {
    return presentation('已经停用', 'neutral', accountNotice);
  }
  if (accountNotice?.blocksExecution) {
    return presentation(
      accountNotice.statusLabel,
      accountNotice.tone,
      accountNotice,
      true
    );
  }
  if (subscription.pending_run) {
    return presentation('等待追加运行', 'warning', accountNotice);
  }

  const state = subscription.recent_state;
  if (state === 'failed') {
    return presentation(
      subscriptionStateLabel(state),
      'error',
      accountNotice,
      true
    );
  }
  if (state === 'paused') {
    return presentation(
      subscriptionStateLabel(state),
      'warning',
      accountNotice,
      true
    );
  }
  if (state === 'running') {
    return presentation(
      subscriptionStateLabel(state),
      'primary',
      accountNotice
    );
  }
  if (state === 'succeeded') {
    return presentation(
      subscriptionStateLabel(state),
      'success',
      accountNotice
    );
  }
  return presentation(subscriptionStateLabel(state), 'neutral', accountNotice);
}

function presentation(
  label: string,
  tone: SubscriptionStatusTone,
  accountNotice: PixivAccountNotice | null,
  requiresAttention = false
): SubscriptionPresentation {
  return {
    label,
    tone,
    requiresAttention,
    accountTitle: accountNotice?.title ?? null,
    accountMessage: accountNotice?.message ?? null,
    blocksImmediateRun: accountNotice?.blocksExecution ?? false
  };
}
