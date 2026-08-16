import { describe, expect, it, vi } from 'vitest';

import { loadDirectorySnapshot } from './directory-pages';

describe('directory snapshot loading', () => {
  it('loads the current visible depth before replacing a directory', async () => {
    const source = Array.from({ length: 160 }, (_, index) => index + 1);
    const loadPage = vi.fn(
      async (_query: string, cursor: string | null, limit: number) => {
        const start = Number(cursor ?? 0);
        const items = source.slice(start, start + limit);
        const next = start + items.length;
        return {
          items,
          total: source.length,
          nextCursor: next < source.length ? String(next) : null
        };
      }
    );

    const snapshot = await loadDirectorySnapshot(
      loadPage,
      '',
      48,
      source.length
    );

    expect(snapshot).toEqual({
      items: source,
      total: source.length,
      nextCursor: null
    });
    expect(loadPage.mock.calls.map((call) => call[1])).toEqual([
      null,
      '48',
      '96',
      '144'
    ]);
  });

  it('keeps the next cursor when the requested visible depth is reached', async () => {
    const loadPage = vi.fn(
      async (_query: string, cursor: string | null, limit: number) => {
        const start = Number(cursor ?? 0);
        return {
          items: Array.from({ length: limit }, (_, index) => start + index),
          total: 200,
          nextCursor: String(start + limit)
        };
      }
    );

    const snapshot = await loadDirectorySnapshot(loadPage, '', 48, 96);

    expect(snapshot.items).toHaveLength(96);
    expect(snapshot.nextCursor).toBe('96');
  });
});
