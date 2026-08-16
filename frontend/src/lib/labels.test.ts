import { describe, expect, expectTypeOf, it } from 'vitest';

import type { components } from './api/schema';
import type { AccountState, JobPriority } from './api/system';
import {
  accountStateLabel,
  subscriptionStateLabel,
  TASK_PRIORITIES,
  taskPriorityLabel
} from './labels';

type SubscriptionRecentState = components['schemas']['SubscriptionRecentState'];

describe('closed state labels', () => {
  it('accepts the generated state and priority unions', () => {
    expectTypeOf(accountStateLabel).parameter(0).toEqualTypeOf<AccountState>();
    expectTypeOf(subscriptionStateLabel)
      .parameter(0)
      .toEqualTypeOf<SubscriptionRecentState>();
    expectTypeOf(taskPriorityLabel).parameter(0).toEqualTypeOf<JobPriority>();
  });

  it('provides one ordered label for every task priority', () => {
    expect(TASK_PRIORITIES.map(taskPriorityLabel)).toEqual([
      '即时操作',
      '手动导入',
      '定时采集',
      '后台维护'
    ]);
  });

  it('labels every account and subscription state', () => {
    const accountStates: AccountState[] = [
      'unconfigured',
      'validating',
      'normal',
      'restricted',
      'credential_invalid'
    ];
    const subscriptionStates: SubscriptionRecentState[] = [
      'never_run',
      'running',
      'succeeded',
      'failed',
      'paused'
    ];

    expect(accountStates.map(accountStateLabel)).toEqual([
      '未配置',
      '验证中',
      '正常',
      '受限访问',
      '凭据失效'
    ]);
    expect(subscriptionStates.map(subscriptionStateLabel)).toEqual([
      '尚未运行',
      '正在运行',
      '最近成功',
      '最近失败',
      '已暂停'
    ]);
  });
});
