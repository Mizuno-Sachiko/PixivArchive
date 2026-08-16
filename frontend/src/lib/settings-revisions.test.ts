import { describe, expect, it } from 'vitest';

import type { SystemStatus } from '$lib/api/system';
import { withSettingRevision } from './settings-revisions';

describe('system setting revisions', () => {
  it('uses the revision returned by a successful save without losing other groups', () => {
    const status: SystemStatus = {
      version: '0.1.0',
      git_commit: null,
      migration_version: 18,
      database: { status: 'ok', message: null },
      media: { status: 'ok', message: null },
      worker: { status: 'ok', message: null },
      storage: {
        active_media_root: '/srv/pixivarchive/media',
        total_bytes: 1_000,
        available_bytes: 800,
        warning_threshold_bytes: 200,
        write_stop_threshold_bytes: 100,
        write_stopped: false
      },
      capabilities: {
        webp_derivatives: true,
        avif_derivatives: false,
        reflink: true
      },
      queue: {},
      setting_revisions: { queue: 3, storage: 8 }
    };

    const updated = withSettingRevision(status, {
      group: 'queue',
      revision: 4
    });

    expect(updated.setting_revisions).toEqual({ queue: 4, storage: 8 });
    expect(status.setting_revisions.queue).toBe(3);
  });
});
