import { describe, expect, it } from 'vitest';

import {
  isPixivAccountAvailable,
  pixivAccountFavoritesNotice,
  pixivAccountNotice
} from './pixiv-account-status';

describe('Pixiv account notice', () => {
  it('explains how expired credentials affect work and recover', () => {
    expect(pixivAccountNotice('credential_invalid')).toEqual({
      title: 'Pixiv账户需要重新验证',
      statusLabel: '等待账户恢复',
      message:
        'Cookie已经失效，依赖该账户的订阅和任务会保持等待。更新并验证Cookie后会自动继续。',
      tone: 'error',
      blocksExecution: true
    });
  });

  it('distinguishes validation from restricted content access', () => {
    expect(pixivAccountNotice('validating')).toMatchObject({
      statusLabel: '等待账户验证',
      tone: 'warning',
      blocksExecution: true
    });
    expect(pixivAccountNotice('restricted')).toMatchObject({
      statusLabel: '访问受限',
      tone: 'warning',
      blocksExecution: false
    });
  });

  it('does not show a notice for a healthy account', () => {
    expect(pixivAccountNotice('normal')).toBeNull();
  });

  it('shows identity and account projections only for usable states', () => {
    expect(isPixivAccountAvailable('normal')).toBe(true);
    expect(isPixivAccountAvailable('restricted')).toBe(true);
    expect(isPixivAccountAvailable('unconfigured')).toBe(false);
    expect(isPixivAccountAvailable('validating')).toBe(false);
    expect(isPixivAccountAvailable('credential_invalid')).toBe(false);
  });

  it.each([
    [
      'unconfigured',
      '尚未配置Pixiv账户',
      '配置并验证Pixiv Cookie后，才能查看这个账户的收藏。'
    ],
    ['validating', 'Pixiv账户正在验证', '验证完成后会显示这个账户的收藏。'],
    [
      'credential_invalid',
      'Pixiv账户需要重新验证',
      '更新并验证Pixiv Cookie后，才能继续查看这个账户的收藏。'
    ]
  ] as const)(
    'describes the unavailable favorites projection for %s',
    (state, title, message) => {
      expect(pixivAccountFavoritesNotice(state)).toMatchObject({
        title,
        message,
        blocksExecution: true
      });
    }
  );

  it.each(['normal', 'restricted'] as const)(
    'keeps favorites available for %s',
    (state) => {
      expect(pixivAccountFavoritesNotice(state)).toBeNull();
    }
  );
});
