import { describe, expect, it } from 'vitest';

import {
  createGalleryQueryStore,
  createGalleryQuery,
  serializeGalleryQuery
} from './gallery-query.svelte';

describe('gallery query state', () => {
  it('serializes structured text and tag groups without a stale cursor', () => {
    const query = createGalleryQuery();
    query.cursor = {
      sort_field: 'pixiv_id',
      sort_direction: 'descending',
      key: { type: 'integer', value: 200 },
      work_id: '0198f64c-42a2-7374-bace-9f1c3b317fb0'
    };
    query.groups = [
      {
        mode: 'all',
        filters: [
          {
            type: 'text',
            field: 'title',
            operator: 'contains',
            value: ' 星 '
          },
          {
            type: 'tags',
            operator: 'all',
            names: ['風景', '夜空'],
            scope: 'original_and_translation'
          }
        ]
      }
    ];

    expect(serializeGalleryQuery(query)).toEqual({
      group_mode: 'all',
      groups: [
        {
          mode: 'all',
          filters: [
            {
              type: 'text',
              field: 'title',
              operator: 'contains',
              value: '星'
            },
            {
              type: 'tags',
              operator: 'all',
              names: ['風景', '夜空'],
              scope: 'original_and_translation'
            }
          ]
        }
      ],
      sort_field: 'pixiv_id',
      sort_direction: 'descending',
      limit: 60
    });
  });

  it('keeps the sort field and direction independent', () => {
    const query = createGalleryQuery();
    query.sort_field = 'title';
    query.sort_direction = 'ascending';

    expect(serializeGalleryQuery(query)).toMatchObject({
      sort_field: 'title',
      sort_direction: 'ascending'
    });
  });

  it('adds an explicit AI classification filter', () => {
    const query = createGalleryQueryStore();
    query.aiGenerated = 'no';

    expect(query.build().groups).toEqual([
      {
        mode: 'all',
        filters: [{ type: 'boolean', field: 'ai_generated', value: false }]
      }
    ]);
  });

  it('builds bookmark count filters from optional range bounds', () => {
    const query = createGalleryQueryStore();
    expect(query.build().groups).toEqual([]);

    query.minimumBookmarks = 100;
    expect(query.build().groups[0].filters).toContainEqual({
      type: 'number',
      field: 'bookmark_count',
      comparison: { operator: 'greater_than_or_equal', value: 100 }
    });

    query.reset();
    query.maximumBookmarks = 500;
    expect(query.build().groups[0].filters).toContainEqual({
      type: 'number',
      field: 'bookmark_count',
      comparison: { operator: 'less_than_or_equal', value: 500 }
    });

    query.minimumBookmarks = 100;
    expect(query.build().groups[0].filters).toContainEqual({
      type: 'number',
      field: 'bookmark_count',
      comparison: {
        operator: 'between',
        value: { min: 100, max: 500 }
      }
    });

    query.minimumBookmarks = 600;
    expect(query.validationError).toBe('收藏数下限不能大于上限');
  });

  it('uses only positive safe integers as Pixiv work IDs', () => {
    const query = createGalleryQueryStore();
    query.searchText = '120001';
    expect(query.build().groups[0].filters).toEqual([
      { type: 'pixiv_work_id', value: 120001 }
    ]);

    query.searchText = String(Number.MAX_SAFE_INTEGER + 1);
    expect(query.build().groups[0].filters).toEqual([
      {
        type: 'text',
        field: 'any',
        operator: 'contains',
        value: String(Number.MAX_SAFE_INTEGER + 1)
      }
    ]);
  });
});
