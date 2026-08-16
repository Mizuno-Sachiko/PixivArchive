import { describe, expect, it } from 'vitest';

import type { GalleryWork } from '$lib/api/gallery';

import { createGalleryQueryStore } from './gallery-query.svelte';
import {
  clearGalleryReturn,
  saveGalleryReturn,
  takeGalleryReturn
} from './gallery-return';

describe('gallery return cache', () => {
  it('keeps loaded gallery data in memory without serializing it into session storage', () => {
    const route = '/gallery/test-return';
    const items = [{ id: 'work-1' }] as unknown as GalleryWork[];
    const query = createGalleryQueryStore();
    const appliedQuery = query.build();
    query.searchText = '尚未提交的输入';
    saveGalleryReturn({
      route,
      scrollY: 420,
      items,
      totalCount: 1,
      query,
      appliedQuery
    });

    const restored = takeGalleryReturn(route);
    expect(restored?.scrollY).toBe(420);
    expect(restored?.items).toBe(items);
    expect(restored?.appliedQuery.groups).toEqual([]);
    expect(restored?.query.build().groups).toHaveLength(1);
    clearGalleryReturn(route);
    expect(takeGalleryReturn(route)).toBeNull();
  });
});
