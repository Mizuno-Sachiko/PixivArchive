import { describe, expect, it } from 'vitest';

import {
  navigationSectionFromPath,
  primaryNavigationItems,
  searchNavigationPages,
  secondaryNavigationItems
} from './navigation';

describe('navigation registry', () => {
  it('derives primary and secondary navigation from the same hierarchy', () => {
    expect(primaryNavigationItems).toEqual([
      { label: '概览', href: '/overview', section: 'overview' },
      { label: '图库', href: '/gallery', section: 'gallery' },
      {
        label: '发现',
        href: '/discovery/subscriptions',
        section: 'discovery'
      },
      { label: '规则', href: '/rules', section: 'rules' },
      { label: '任务', href: '/tasks', section: 'tasks' },
      { label: '系统', href: '/system/account', section: 'system' }
    ]);
    expect(secondaryNavigationItems('gallery')).toEqual([
      { label: '全部作品', href: '/gallery' },
      { label: '收藏', href: '/gallery/favorites' },
      { label: '作者', href: '/gallery/artists' },
      { label: '标签', href: '/gallery/tags' },
      { label: '系列', href: '/gallery/series' }
    ]);
  });

  it('maps contextual routes to their owning section', () => {
    expect(navigationSectionFromPath('/gallery/works/120001')).toBe('gallery');
    expect(navigationSectionFromPath('/system/settings')).toBe('system');
    expect(navigationSectionFromPath('/')).toBe('overview');
  });

  it('searches page names and their navigation hierarchy', () => {
    expect(searchNavigationPages('收藏')).toEqual([
      {
        key: 'page:/gallery/favorites',
        kind: 'page',
        label: '收藏',
        detail: '图库 / 收藏',
        href: '/gallery/favorites',
        icon: 'archive'
      }
    ]);

    const galleryPages = searchNavigationPages('图库');
    expect(galleryPages.map((page) => page.label)).toEqual([
      '全部作品',
      '收藏',
      '作者',
      '标签',
      '系列'
    ]);
  });

  it('returns one primary destination per section for an empty query', () => {
    const pages = searchNavigationPages('');

    expect(pages.map(({ label, href }) => ({ label, href }))).toEqual([
      { label: '系统概况', href: '/overview' },
      { label: '全部作品', href: '/gallery' },
      { label: '订阅计划', href: '/discovery/subscriptions' },
      { label: '规则工作台', href: '/rules' },
      { label: '运行记录', href: '/tasks' },
      { label: 'Pixiv账户', href: '/system/account' }
    ]);
  });
});
