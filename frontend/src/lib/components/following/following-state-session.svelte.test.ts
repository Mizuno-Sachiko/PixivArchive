import { describe, expect, it, vi } from 'vitest';

import type {
  FollowingApi,
  FollowingAuthor,
  FollowingState
} from '$lib/api/following';
import type { Subscription } from '$lib/api/subscriptions';

import { FollowingStateSession } from './following-state-session.svelte';

const accountA = '019fe98e-0e22-7000-8000-000000000001';
const accountB = '019fe98e-0e22-7000-8000-000000000002';

describe('FollowingStateSession', () => {
  it('preserves an author intent while an older load is being applied', async () => {
    const reload = deferred<FollowingState>();
    const authorUpdate = deferred<FollowingAuthor>();
    let loadCount = 0;
    const updateAuthor = vi.fn(() => authorUpdate.promise);
    const session = new FollowingStateSession(
      api({
        get: () =>
          loadCount++ === 0
            ? Promise.resolve(followingState(accountA))
            : reload.promise,
        updateAuthor
      })
    );
    session.setAccount(accountA);
    await session.load();

    const reloading = session.load();
    await vi.waitFor(() => expect(loadCount).toBe(2));
    const saving = session.updateAuthor(70001, false);
    expect(session.state?.authors[0]?.enabled).toBe(false);

    reload.resolve(followingState(accountA));
    await vi.waitFor(() => expect(updateAuthor).toHaveBeenCalledOnce());
    expect(session.state?.authors[0]?.enabled).toBe(false);

    authorUpdate.resolve(author(70001, false));
    await Promise.all([reloading, saving]);
    expect(session.state?.authors[0]?.enabled).toBe(false);
  });

  it('preserves a subscription intent when a batch response replaces server state', async () => {
    const batchUpdate = deferred<FollowingState>();
    const subscriptionUpdate = deferred<Subscription>();
    const updateAuthors = vi.fn(() => batchUpdate.promise);
    const updateSubscription = vi.fn(() => subscriptionUpdate.promise);
    const session = loadedSession(
      api({
        get: async () => followingState(accountA),
        updateAuthors,
        updateSubscription
      })
    );
    await session.load();

    const batching = session.updateAuthors([70001], false);
    await vi.waitFor(() => expect(updateAuthors).toHaveBeenCalledOnce());
    const saving = session.updateSubscription(true, 30);
    expect(session.state?.subscription).toMatchObject({
      enabled: true,
      schedule: { interval_minutes: 30 }
    });

    batchUpdate.resolve({
      ...followingState(accountA, false, 2),
      authors: [author(70001, false)]
    });
    await vi.waitFor(() => expect(updateSubscription).toHaveBeenCalledOnce());
    expect(updateSubscription).toHaveBeenCalledWith(true, 30, 2, accountA);
    expect(session.state?.subscription).toMatchObject({
      enabled: true,
      schedule: { interval_minutes: 30 }
    });

    subscriptionUpdate.resolve(subscription(accountA, true, 30, 3));
    await Promise.all([batching, saving]);
  });

  it('preserves an author intent when a Pixiv refresh returns older state', async () => {
    const refresh = deferred<FollowingState>();
    const authorUpdate = deferred<FollowingAuthor>();
    const refreshFollowing = vi.fn(() => refresh.promise);
    const updateAuthor = vi.fn(() => authorUpdate.promise);
    const session = loadedSession(
      api({
        get: async () => followingState(accountA),
        refresh: refreshFollowing,
        updateAuthor
      })
    );
    await session.load();

    const refreshing = session.refresh();
    await vi.waitFor(() => expect(refreshFollowing).toHaveBeenCalledOnce());
    const saving = session.updateAuthor(70001, false);
    expect(session.state?.authors[0]?.enabled).toBe(false);

    refresh.resolve(followingState(accountA));
    await vi.waitFor(() => expect(updateAuthor).toHaveBeenCalledOnce());
    expect(session.state?.authors[0]?.enabled).toBe(false);

    authorUpdate.resolve(author(70001, false));
    await Promise.all([refreshing, saving]);
  });

  it('ignores an account response that finishes after the next account loads', async () => {
    const accountALoad = deferred<FollowingState>();
    let loadCount = 0;
    const get = vi.fn(() =>
      loadCount++ === 0
        ? accountALoad.promise
        : Promise.resolve(followingState(accountB))
    );
    const session = new FollowingStateSession(api({ get }));
    session.setAccount(accountA);

    const loadingA = session.load();
    await vi.waitFor(() => expect(get).toHaveBeenCalledOnce());
    session.setAccount(accountB);
    await session.load();
    expect(session.state?.subscription.account_id).toBe(accountB);

    accountALoad.resolve(followingState(accountA));
    await loadingA;
    expect(session.state?.subscription.account_id).toBe(accountB);
  });

  it('suppresses a refresh failure from an account that is no longer current', async () => {
    const accountARefresh = deferred<FollowingState>();
    const refresh = vi.fn(() => accountARefresh.promise);
    let currentAccount = accountA;
    const session = new FollowingStateSession(
      api({
        get: async () => followingState(currentAccount),
        refresh
      })
    );
    session.setAccount(accountA);
    await session.load();

    const refreshingA = session.refresh();
    await vi.waitFor(() => expect(refresh).toHaveBeenCalledOnce());
    currentAccount = accountB;
    session.setAccount(accountB);
    await session.load();

    accountARefresh.reject(new Error('account A request failed'));
    await expect(refreshingA).resolves.toBe(false);
    expect(session.state?.subscription.account_id).toBe(accountB);
  });
});

function loadedSession(apiClient: FollowingApi): FollowingStateSession {
  const session = new FollowingStateSession(apiClient);
  session.setAccount(accountA);
  return session;
}

function api(overrides: Partial<FollowingApi>): FollowingApi {
  const unexpected = async (): Promise<never> => {
    throw new Error('unexpected API request');
  };
  return {
    get: unexpected,
    updateSubscription: unexpected,
    updateAuthor: unexpected,
    updateAuthors: unexpected,
    artistFollowState: unexpected,
    updateArtistFollow: unexpected,
    run: unexpected,
    refresh: unexpected,
    ...overrides
  };
}

function followingState(
  accountId: string,
  enabled = false,
  revision = 1
): FollowingState {
  return {
    subscription: subscription(accountId, enabled, 15, revision),
    authors: [author(70001, true)],
    last_full_reconciled_at: null
  };
}

function subscription(
  accountId: string,
  enabled: boolean,
  intervalMinutes: number,
  revision: number
): Subscription {
  return {
    id: `subscription-${accountId}`,
    account_id: accountId,
    account_pixiv_user_id: 60001,
    account_avatar_url: null,
    account_state: 'normal',
    enabled,
    kind: 'following',
    name: '关注订阅',
    next_run_at: null,
    params: {},
    pending_run: false,
    recent_state: 'never_run',
    revision,
    rule_id: null,
    schedule: {
      interval_minutes: intervalMinutes,
      lookback_pages: 1
    }
  };
}

function author(pixivArtistId: number, enabled: boolean): FollowingAuthor {
  return {
    pixiv_artist_id: pixivArtistId,
    display_name: `作者${pixivArtistId}`,
    avatar_url: null,
    enabled,
    last_collected_at: null,
    refreshed_at: '2026-08-16T00:00:00Z',
    visibility: 'visible'
  };
}

function deferred<T>(): {
  promise: Promise<T>;
  resolve: (value: T) => void;
  reject: (cause: unknown) => void;
} {
  let resolve!: (value: T) => void;
  let reject!: (cause: unknown) => void;
  const promise = new Promise<T>((onResolve, onReject) => {
    resolve = onResolve;
    reject = onReject;
  });
  return { promise, resolve, reject };
}
