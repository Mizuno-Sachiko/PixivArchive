import { describe, expect, it } from 'vitest';

import {
  createGalleryQuery,
  createGalleryTextSearch,
  galleryTextFilter
} from './gallery-search';

describe('gallery text search', () => {
  it('uses an exact Pixiv work ID filter for a positive integer', () => {
    expect(galleryTextFilter(' 00120001 ')).toEqual({
      type: 'pixiv_work_id',
      value: 120001
    });
  });

  it('uses the shared text filter for other non-empty input', () => {
    expect(galleryTextFilter(' 夜空 ')).toEqual({
      type: 'text',
      field: 'any',
      operator: 'contains',
      value: '夜空'
    });
    expect(galleryTextFilter('0')).toEqual({
      type: 'text',
      field: 'any',
      operator: 'contains',
      value: '0'
    });
  });

  it('omits a filter for whitespace-only input', () => {
    expect(galleryTextFilter('   ')).toBeNull();
  });

  it('builds the bounded work query from the default gallery contract', () => {
    expect(createGalleryTextSearch('夜空', 5)).toEqual({
      ...createGalleryQuery(),
      groups: [
        {
          mode: 'all',
          filters: [
            {
              type: 'text',
              field: 'any',
              operator: 'contains',
              value: '夜空'
            }
          ]
        }
      ],
      limit: 5
    });
  });
});
