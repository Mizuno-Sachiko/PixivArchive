import { afterEach, describe, expect, it, vi } from 'vitest';

import {
  GalleryRefreshCoordinator,
  refreshVisibleItems
} from './gallery-refresh';

const initial = {
  work: 0,
  pixivBookmark: 0,
  pixivAccount: 0,
  snapshot: 0
};

describe('gallery refresh coordinator', () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it('coalesces resource changes and ignores an unchanged version', async () => {
    vi.useFakeTimers();
    const refresh = vi.fn(async () => true);
    const coordinator = new GalleryRefreshCoordinator(initial, refresh);

    coordinator.observe(initial, false);
    coordinator.observe({ ...initial, work: 1 }, false);
    coordinator.observe({ ...initial, work: 2 }, false);
    await vi.advanceTimersByTimeAsync(499);
    expect(refresh).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(1);

    expect(refresh).toHaveBeenCalledTimes(1);
    coordinator.dispose();
  });

  it('refreshes after a full snapshot invalidation', async () => {
    vi.useFakeTimers();
    const refresh = vi.fn(async () => true);
    const coordinator = new GalleryRefreshCoordinator(initial, refresh);

    coordinator.observe({ ...initial, snapshot: 1 }, false);
    await vi.advanceTimersByTimeAsync(500);

    expect(refresh).toHaveBeenCalledTimes(1);
    coordinator.dispose();
  });

  it('waits for selection mode to end before refreshing', async () => {
    vi.useFakeTimers();
    const refresh = vi.fn(async () => true);
    const coordinator = new GalleryRefreshCoordinator(initial, refresh);
    const changed = { ...initial, work: 1 };

    coordinator.observe(changed, true);
    await vi.advanceTimersByTimeAsync(1_000);
    expect(refresh).not.toHaveBeenCalled();
    coordinator.observe(changed, false);
    await vi.advanceTimersByTimeAsync(500);

    expect(refresh).toHaveBeenCalledTimes(1);
    coordinator.dispose();
  });

  it('runs one follow-up when versions change during a refresh', async () => {
    vi.useFakeTimers();
    const first = deferred<void>();
    let calls = 0;
    const coordinator = new GalleryRefreshCoordinator(initial, async () => {
      calls += 1;
      if (calls === 1) await first.promise;
      return true;
    });

    coordinator.observe({ ...initial, work: 1 }, false);
    await vi.advanceTimersByTimeAsync(500);
    coordinator.observe({ ...initial, work: 2 }, false);
    coordinator.observe({ ...initial, work: 3 }, false);
    first.resolve();
    await Promise.resolve();
    await vi.advanceTimersByTimeAsync(500);

    expect(calls).toBe(2);
    coordinator.dispose();
  });

  it('keeps a newer observed version when a manual query marks its start version current', async () => {
    vi.useFakeTimers();
    const refresh = vi.fn(async () => true);
    const coordinator = new GalleryRefreshCoordinator(initial, refresh);
    const changed = { ...initial, work: 1 };

    coordinator.observe(changed, true);
    coordinator.markCurrent(initial);
    coordinator.observe(changed, false);
    await vi.advanceTimersByTimeAsync(500);

    expect(refresh).toHaveBeenCalledTimes(1);
    coordinator.dispose();
  });

  it('waits for an explicit retry after a resource refresh fails', async () => {
    vi.useFakeTimers();
    const refresh = vi
      .fn<() => Promise<boolean>>()
      .mockResolvedValueOnce(false)
      .mockResolvedValueOnce(true);
    const coordinator = new GalleryRefreshCoordinator(initial, refresh);
    const changed = { ...initial, work: 1 };

    coordinator.observe(changed, false);
    await vi.advanceTimersByTimeAsync(500);
    await vi.advanceTimersByTimeAsync(1_000);
    expect(refresh).toHaveBeenCalledTimes(1);

    coordinator.retry();
    await Promise.resolve();

    expect(refresh).toHaveBeenCalledTimes(2);
    coordinator.dispose();
  });
});

describe('visible gallery refresh', () => {
  it('updates retained cards and removes items absent from the refreshed result', () => {
    const result = refreshVisibleItems(
      [
        { id: 'first', title: 'old first' },
        { id: 'second', title: 'old second' }
      ],
      [
        { id: 'new', title: 'new' },
        { id: 'first', title: 'updated first' }
      ]
    );

    expect(result).toEqual([{ id: 'first', title: 'updated first' }]);
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
