import { describe, expect, it, vi } from 'vitest';

import type { EventResource } from '$lib/api/events';

import {
  AppEventRefreshCoordinator,
  composeAppEventVersion
} from './app-event-refresh';

const revisions: Record<EventResource, number> = {
  job: 1,
  rule: 2,
  work: 3,
  pixiv_bookmark: 9,
  pixiv_account: 4,
  deletion_marker: 5,
  subscription: 6,
  system_setting: 7,
  administrator: 8
};

describe('app event refresh coordinator', () => {
  it('includes full snapshots and only the selected resources in a version', () => {
    expect(
      composeAppEventVersion(9, revisions, ['work', 'pixiv_account'])
    ).toBe('9:3:4');
  });

  it('loads initially and follows a version that changes during the request', async () => {
    const first = deferred<boolean>();
    const refresh = vi
      .fn<() => Promise<boolean>>()
      .mockImplementationOnce(() => first.promise)
      .mockResolvedValueOnce(true);
    const coordinator = new AppEventRefreshCoordinator(refresh);

    coordinator.start('0:0');
    coordinator.observe('1:0');
    first.resolve(true);
    await vi.waitFor(() => expect(refresh).toHaveBeenCalledTimes(2));

    coordinator.dispose();
  });

  it('waits for an explicit retry after the current version fails', async () => {
    const refresh = vi
      .fn<() => Promise<boolean>>()
      .mockResolvedValueOnce(false)
      .mockResolvedValueOnce(true);
    const coordinator = new AppEventRefreshCoordinator(refresh);

    coordinator.start('0:0');
    await vi.waitFor(() => expect(refresh).toHaveBeenCalledTimes(1));
    coordinator.observe('0:0');
    await Promise.resolve();
    expect(refresh).toHaveBeenCalledTimes(1);

    coordinator.retry();
    await vi.waitFor(() => expect(refresh).toHaveBeenCalledTimes(2));
    coordinator.dispose();
  });

  it('honors a retry requested while the failing refresh is still finishing', async () => {
    const first = deferred<boolean>();
    const refresh = vi
      .fn<() => Promise<boolean>>()
      .mockImplementationOnce(() => first.promise)
      .mockResolvedValueOnce(true);
    const coordinator = new AppEventRefreshCoordinator(refresh);

    coordinator.start('0:0');
    await vi.waitFor(() => expect(refresh).toHaveBeenCalledTimes(1));
    coordinator.retry();
    first.resolve(false);

    await vi.waitFor(() => expect(refresh).toHaveBeenCalledTimes(2));
    coordinator.dispose();
  });
});

function deferred<T>(): {
  promise: Promise<T>;
  resolve: (value: T) => void;
} {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((next) => {
    resolve = next;
  });
  return { promise, resolve };
}
