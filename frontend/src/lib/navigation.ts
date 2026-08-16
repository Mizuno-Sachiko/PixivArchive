import type { Pathname } from '$app/types';

export type NavigationSection =
  'overview' | 'gallery' | 'discovery' | 'rules' | 'tasks' | 'system';

export type NavigationIcon =
  'archive' | 'check' | 'database' | 'queue' | 'search' | 'storage';

interface NavigationPage {
  label: string;
  href: Pathname;
  aliases?: readonly string[];
}

interface NavigationSectionDefinition {
  key: NavigationSection;
  label: string;
  icon: NavigationIcon;
  pages: readonly NavigationPage[];
}

export interface NavigationPageResult {
  key: `page:${Pathname}`;
  kind: 'page';
  label: string;
  detail: string;
  href: Pathname;
  icon: NavigationIcon;
}

const navigationSections = [
  {
    key: 'overview',
    label: '概览',
    icon: 'database',
    pages: [{ label: '系统概况', href: '/overview', aliases: ['概览', '状态'] }]
  },
  {
    key: 'gallery',
    label: '图库',
    icon: 'archive',
    pages: [
      { label: '全部作品', href: '/gallery', aliases: ['作品'] },
      { label: '收藏', href: '/gallery/favorites' },
      { label: '作者', href: '/gallery/artists' },
      { label: '标签', href: '/gallery/tags' },
      { label: '系列', href: '/gallery/series' }
    ]
  },
  {
    key: 'discovery',
    label: '发现',
    icon: 'search',
    pages: [
      { label: '订阅计划', href: '/discovery/subscriptions' },
      {
        label: '关注订阅',
        href: '/discovery/following',
        aliases: ['关注作者']
      },
      {
        label: '排行榜订阅',
        href: '/discovery/rankings',
        aliases: ['排行榜']
      },
      { label: '手动导入', href: '/discovery/imports', aliases: ['导入'] }
    ]
  },
  {
    key: 'rules',
    label: '规则',
    icon: 'check',
    pages: [{ label: '规则工作台', href: '/rules', aliases: ['规则'] }]
  },
  {
    key: 'tasks',
    label: '任务',
    icon: 'queue',
    pages: [
      { label: '运行记录', href: '/tasks', aliases: ['任务'] },
      { label: '下载队列', href: '/tasks/downloads' },
      { label: '错误记录', href: '/tasks/errors' }
    ]
  },
  {
    key: 'system',
    label: '系统',
    icon: 'storage',
    pages: [
      { label: 'Pixiv账户', href: '/system/account', aliases: ['账号'] },
      { label: '回收站', href: '/system/trash' },
      { label: '系统设置', href: '/system/settings', aliases: ['设置'] },
      { label: '关于', href: '/system/about' }
    ]
  }
] as const satisfies readonly NavigationSectionDefinition[];

export const primaryNavigationItems = navigationSections.map((section) => ({
  label: section.label,
  href: section.pages[0].href,
  section: section.key
}));

export function navigationSectionFromPath(pathname: string): NavigationSection {
  const candidate = pathname.split('/').filter(Boolean)[0];
  return (
    navigationSections.find((section) => section.key === candidate)?.key ??
    'overview'
  );
}

export function secondaryNavigationItems(
  section: NavigationSection
): ReadonlyArray<{ label: string; href: Pathname }> {
  return navigationSections
    .find((item) => item.key === section)!
    .pages.map(({ label, href }) => ({ label, href }));
}

export function searchNavigationPages(value: string): NavigationPageResult[] {
  const query = normalize(value);
  if (!query) {
    return navigationSections.map((section) =>
      navigationPageResult(section, section.pages[0])
    );
  }
  return navigationSections.flatMap((section) =>
    section.pages
      .filter((page) => {
        const aliases = 'aliases' in page ? page.aliases : [];
        return normalize(
          [section.label, page.label, ...aliases].join(' ')
        ).includes(query);
      })
      .map((page) => navigationPageResult(section, page))
  );
}

function navigationPageResult(
  section: NavigationSectionDefinition,
  page: NavigationPage
): NavigationPageResult {
  return {
    key: `page:${page.href}`,
    kind: 'page',
    label: page.label,
    detail: `${section.label} / ${page.label}`,
    href: page.href,
    icon: section.icon
  };
}

function normalize(value: string): string {
  return value.trim().toLocaleLowerCase();
}
