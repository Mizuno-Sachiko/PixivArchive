import type { AccountState } from '$lib/api/system';

export interface PixivAccountNotice {
  title: string;
  statusLabel: string;
  message: string;
  tone: 'warning' | 'error';
  blocksExecution: boolean;
}

export function isPixivAccountAvailable(state: AccountState): boolean {
  switch (state) {
    case 'normal':
    case 'restricted':
      return true;
    case 'unconfigured':
    case 'validating':
    case 'credential_invalid':
      return false;
  }
}

export function pixivAccountNotice(
  state: AccountState
): PixivAccountNotice | null {
  switch (state) {
    case 'credential_invalid':
      return {
        title: 'Pixiv账户需要重新验证',
        statusLabel: '等待账户恢复',
        message:
          'Cookie已经失效，依赖该账户的订阅和任务会保持等待。更新并验证Cookie后会自动继续。',
        tone: 'error',
        blocksExecution: true
      };
    case 'validating':
      return {
        title: 'Pixiv账户正在验证',
        statusLabel: '等待账户验证',
        message: '验证完成前，依赖该账户的订阅和任务会保持等待。',
        tone: 'warning',
        blocksExecution: true
      };
    case 'unconfigured':
      return {
        title: '尚未配置Pixiv账户',
        statusLabel: '等待账户配置',
        message: '排行榜、关注、收藏和手动导入暂时无法执行。',
        tone: 'warning',
        blocksExecution: true
      };
    case 'restricted':
      return {
        title: 'Pixiv账户访问受限',
        statusLabel: '访问受限',
        message: '当前账户无法访问R-18内容，其他采集任务可以继续执行。',
        tone: 'warning',
        blocksExecution: false
      };
    case 'normal':
      return null;
  }
}

export function pixivAccountFavoritesNotice(
  state: AccountState
): PixivAccountNotice | null {
  switch (state) {
    case 'unconfigured':
      return {
        ...requiredAccountNotice(state),
        message: '配置并验证Pixiv Cookie后，才能查看这个账户的收藏。'
      };
    case 'validating':
      return {
        ...requiredAccountNotice(state),
        message: '验证完成后会显示这个账户的收藏。'
      };
    case 'credential_invalid':
      return {
        ...requiredAccountNotice(state),
        message: '更新并验证Pixiv Cookie后，才能继续查看这个账户的收藏。'
      };
    case 'normal':
    case 'restricted':
      return null;
  }
}

function requiredAccountNotice(state: AccountState): PixivAccountNotice {
  const notice = pixivAccountNotice(state);
  if (!notice) {
    throw new Error(`account state ${state} does not require a notice`);
  }
  return notice;
}
