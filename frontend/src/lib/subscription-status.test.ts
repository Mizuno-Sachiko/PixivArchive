import { describe, expect, it } from 'vitest';

import type { Subscription } from '$lib/api/subscriptions';
import { subscriptionPresentation } from './subscription-status';

describe('subscription presentation', () => {
  it('derives a clear waiting state from an invalid account', () => {
    const status = subscriptionPresentation(
      subscription({ account_state: 'credential_invalid' })
    );

    expect(status.label).toBe('等待账户恢复');
    expect(status.tone).toBe('error');
    expect(status.requiresAttention).toBe(true);
    expect(status.accountMessage).toContain('Cookie已经失效');
    expect(status.blocksImmediateRun).toBe(true);
  });

  it('distinguishes account validation from expired credentials', () => {
    const status = subscriptionPresentation(
      subscription({ account_state: 'validating' })
    );

    expect(status.label).toBe('等待账户验证');
    expect(status.tone).toBe('warning');
    expect(status.requiresAttention).toBe(true);
    expect(status.blocksImmediateRun).toBe(true);
  });

  it('keeps a disabled subscription neutral', () => {
    const status = subscriptionPresentation(
      subscription({ enabled: false, account_state: 'credential_invalid' })
    );

    expect(status).toMatchObject({
      label: '已经停用',
      tone: 'neutral',
      requiresAttention: false,
      accountMessage: expect.stringContaining('Cookie已经失效'),
      blocksImmediateRun: true
    });
  });

  it('warns about restricted content while keeping collection available', () => {
    const status = subscriptionPresentation(
      subscription({ account_state: 'restricted' })
    );

    expect(status.accountMessage).toContain('R-18');
    expect(status.blocksImmediateRun).toBe(false);
  });
});

function subscription(overrides: Partial<Subscription> = {}): Subscription {
  return {
    id: '019fe98e-0e22-7000-8000-000000000010',
    account_id: '019fe98e-0e22-7000-8000-000000000001',
    account_pixiv_user_id: 10001,
    account_avatar_url: null,
    account_state: 'normal',
    rule_id: null,
    name: '每日排行榜',
    kind: 'ranking',
    enabled: true,
    schedule: { interval_minutes: 1440, lookback_pages: 2 },
    params: { modes: ['daily'], contents: ['all'] },
    next_run_at: null,
    pending_run: false,
    recent_state: 'succeeded',
    revision: 1,
    ...overrides
  };
}
