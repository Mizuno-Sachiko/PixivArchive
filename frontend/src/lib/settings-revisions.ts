import type { SystemStatus } from '$lib/api/system';

export function withSettingRevision(
  status: SystemStatus,
  saved: { group: string; revision: number }
): SystemStatus {
  return {
    ...status,
    setting_revisions: {
      ...status.setting_revisions,
      [saved.group]: saved.revision
    }
  };
}
