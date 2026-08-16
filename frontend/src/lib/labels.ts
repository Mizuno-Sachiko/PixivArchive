import type { SubscriptionRecentState } from './api/subscriptions';
import type { AccountState, JobPriority } from './api/system';

const SUBSCRIPTION_STATE_LABELS = {
  never_run: '尚未运行',
  running: '正在运行',
  succeeded: '最近成功',
  failed: '最近失败',
  paused: '已暂停'
} as const satisfies Record<SubscriptionRecentState, string>;

export const TASK_PRIORITY_LABELS = {
  immediate: '即时操作',
  manual_import: '手动导入',
  scheduled_collection: '定时采集',
  background_maintenance: '后台维护'
} as const satisfies Record<JobPriority, string>;

export const TASK_PRIORITIES = Object.freeze(
  Object.keys(TASK_PRIORITY_LABELS) as Array<keyof typeof TASK_PRIORITY_LABELS>
);

const ACCOUNT_STATE_LABELS = {
  unconfigured: '未配置',
  validating: '验证中',
  normal: '正常',
  restricted: '受限访问',
  credential_invalid: '凭据失效'
} as const satisfies Record<AccountState, string>;

export function subscriptionKindLabel(kind: string): string {
  return (
    {
      ranking: '排行榜',
      following: '关注作者',
      bookmarks: '收藏'
    }[kind] ?? kind
  );
}

export function subscriptionStateLabel(state: SubscriptionRecentState): string {
  return SUBSCRIPTION_STATE_LABELS[state];
}

export function subscriptionTriggerLabel(trigger: string): string {
  return (
    {
      scheduled: '定时运行',
      manual: '手动运行',
      backfill: '历史补采',
      merged_pending: '合并等待'
    }[trigger] ?? trigger
  );
}

export function taskKindLabel(kind: string): string {
  return (
    {
      ranking_collection: '排行榜采集',
      following_collection: '关注作者采集',
      bookmarks_collection: '收藏采集',
      import_artist: '作者导入',
      import_work: '作品导入',
      download_media: '下载原图',
      generate_derivative: '生成浏览图',
      purge_trash: '清理回收站'
    }[kind] ?? kind
  );
}

export function taskStateLabel(state: string): string {
  return (
    {
      queued: '正在等待',
      running: '正在处理',
      retry_wait: '等待重试',
      waiting_account: '等待Pixiv账户',
      waiting_storage: '等待存储空间',
      completed: '已经完成',
      failed: '失败',
      cancelled: '已取消'
    }[state] ?? state
  );
}

export function taskPriorityLabel(priority: JobPriority): string {
  return TASK_PRIORITY_LABELS[priority];
}

export function errorClassLabel(errorClass: string | null): string {
  if (!errorClass) return '无错误';
  return (
    {
      network: '网络错误',
      server: '来源服务器错误',
      rate_limit: '来源限流',
      credential_invalid: '凭据失效',
      permanent: '永久错误',
      storage: '存储错误',
      processing: '处理错误'
    }[errorClass] ?? errorClass
  );
}

export function accountStateLabel(state: AccountState): string {
  return ACCOUNT_STATE_LABELS[state];
}

export function importKindLabel(kind: string): string {
  return kind === 'artist' ? '作者' : '作品';
}

export function importStateLabel(state: string): string {
  return (
    {
      queued: '正在等待',
      running: '正在采集',
      metadata_saved: '仅保存元数据',
      download_queued: '已经交给下载队列',
      ignored: '已忽略',
      blocked_by_deletion_marker: '受删除标记阻止',
      failed: '失败',
      cancelled: '已取消'
    }[state] ?? state
  );
}
