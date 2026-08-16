import { afterEach, describe, expect, it } from 'vitest';

import type { PixivAccount } from '$lib/api/system';

import { pixivAccountStore } from './pixiv-account.svelte';

describe('PixivAccountStore', () => {
  afterEach(() => pixivAccountStore.reset());

  it('exposes only available accounts for Pixiv actions', () => {
    for (const state of ['normal', 'restricted'] as const) {
      const account = pixivAccount(state);
      pixivAccountStore.replace(account);
      expect(pixivAccountStore.currentForAction).toEqual(account);
    }

    for (const state of [
      'unconfigured',
      'validating',
      'credential_invalid'
    ] as const) {
      pixivAccountStore.replace(pixivAccount(state));
      expect(pixivAccountStore.currentForAction).toBeNull();
    }
  });

  it('keeps an available account actionable during a background refresh', () => {
    const account = pixivAccount('normal');
    pixivAccountStore.replace(account);

    pixivAccountStore.loading = true;

    expect(pixivAccountStore.currentForAction).toEqual(account);
  });
});

function pixivAccount(state: PixivAccount['state']): PixivAccount {
  return {
    account_id: '0198f653-0000-7000-8000-000000000001',
    pixiv_user_id: 10_001,
    display_name: 'Test Artist',
    avatar_url: null,
    state,
    bookmark_writeback_enabled: false,
    last_validated_at: '2026-08-12T00:00:00Z',
    revision: 4
  };
}
