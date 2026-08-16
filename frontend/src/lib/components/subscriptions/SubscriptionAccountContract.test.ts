import { render } from 'svelte/server';
import { describe, expect, it } from 'vitest';

import type { Subscription } from '$lib/api/subscriptions';

import SubscriptionDefinition from './SubscriptionDefinition.svelte';
import SubscriptionTable from './SubscriptionTable.svelte';

describe('subscription account presentation', () => {
  it('renders every numeric account identifier without an unbound-account state', () => {
    const item = subscription({ account_pixiv_user_id: 0 });
    const table = render(SubscriptionTable, {
      props: {
        items: [item],
        selectedId: null,
        onSelect: () => undefined
      }
    });
    const definition = render(SubscriptionDefinition, {
      props: { subscription: item, rules: [] }
    });

    expect(table.body).toContain('Pixiv ID 0');
    expect(table.body).not.toContain('无账户');
    expect(definition.body).toContain('Pixiv ID 0');
    expect(definition.body).not.toContain('未绑定账户');
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
    recent_state: 'never_run',
    revision: 1,
    ...overrides
  };
}
