import { describe, expect, it, vi } from 'vitest';

import type {
  EffectiveSettings,
  SettingUpdate,
  SystemApi
} from '$lib/api/system';

import {
  CollectionSettingsDraft,
  ContentSettingsDraft,
  QueueSettingsDraft,
  StorageSettingsDraft
} from './settings-drafts.svelte';

describe('system setting drafts', () => {
  it('saves queue quotas while preserving stored job priority mappings', async () => {
    const saveSetting = vi
      .fn()
      .mockResolvedValue({ group: 'queue', revision: 5 });
    const original = {
      quota_weights: {
        immediate: 4,
        manual_import: 8,
        scheduled_collection: 2,
        background_maintenance: 1
      },
      job_priorities: jobPriorities()
    };
    const draft = new QueueSettingsDraft(original, 4, {
      saveSetting
    } as unknown as SystemApi);

    draft.backgroundMaintenance = 2;
    await draft.save();

    expect(saveSetting).toHaveBeenCalledWith(
      'queue',
      {
        quota_weights: {
          immediate: 4,
          manual_import: 8,
          scheduled_collection: 2,
          background_maintenance: 2
        },
        job_priorities: jobPriorities()
      },
      4
    );
  });

  it('saves collection defaults as one batch and uses returned revisions next time', async () => {
    const saveSettings = vi
      .fn()
      .mockResolvedValueOnce([
        { group: 'retry', revision: 5 },
        { group: 'derivative', revision: 7 }
      ])
      .mockResolvedValueOnce([
        { group: 'retry', revision: 6 },
        { group: 'derivative', revision: 8 }
      ]);
    const api = { saveSettings } as unknown as SystemApi;
    const draft = new CollectionSettingsDraft(
      { network_backoff_seconds: [60, 300] },
      {
        format: 'webp',
        max_width: 768,
        webp_quality: 80,
        avif_quality: 50
      },
      false,
      { retry: 4, derivative: 6 },
      api
    );

    await draft.save();
    await draft.save();

    expect(saveSettings).toHaveBeenNthCalledWith(1, [
      {
        group: 'retry',
        expected_revision: 4,
        value: { network_backoff_seconds: [60, 300] }
      },
      {
        group: 'derivative',
        expected_revision: 6,
        value: {
          format: 'webp',
          max_width: 768,
          webp_quality: 80,
          avif_quality: 50
        }
      }
    ]);
    const secondUpdates = saveSettings.mock.calls[1][0] as SettingUpdate[];
    expect(secondUpdates.map((update) => update.expected_revision)).toEqual([
      5, 7
    ]);
    expect(draft.message).toBe('采集默认值设置已经保存');
    expect(draft.error).toBe('');
  });

  it('rejects an invalid retry sequence before sending a request', async () => {
    const saveSettings = vi.fn();
    const draft = new CollectionSettingsDraft(
      { network_backoff_seconds: [60, 300] },
      {
        format: 'webp',
        max_width: 768,
        webp_quality: 80,
        avif_quality: 50
      },
      false,
      { retry: 1, derivative: 1 },
      { saveSettings } as unknown as SystemApi
    );

    draft.retryBackoff = '300, 60';
    await draft.save();

    expect(saveSettings).not.toHaveBeenCalled();
    expect(draft.error).toBe('网络错误退避秒数必须是递增的正整数列表');
  });

  it('uses WebP when AVIF derivatives are unavailable', () => {
    const draft = new CollectionSettingsDraft(
      { network_backoff_seconds: [60, 300] },
      {
        format: 'avif',
        max_width: 768,
        webp_quality: 80,
        avif_quality: 50
      },
      false,
      { retry: 1, derivative: 1 },
      { saveSettings: vi.fn() } as unknown as SystemApi
    );

    expect(draft.derivativeFormat).toBe('webp');
  });

  it('saves an absolute media directory and reports the restart requirement', async () => {
    const saveSetting = vi
      .fn()
      .mockResolvedValue({ group: 'storage', revision: 3 });
    const draft = new StorageSettingsDraft(
      {
        media_root: null,
        warning_threshold_bytes: 100 * 1024 ** 3,
        media_write_stop_threshold_bytes: 32 * 1024 ** 3,
        trash_retention_days: 30
      },
      '/srv/pixivarchive/media',
      2,
      { saveSetting } as unknown as SystemApi
    );

    draft.mediaRoot = 'relative/media';
    await draft.save();
    expect(saveSetting).not.toHaveBeenCalled();
    expect(draft.error).toBe('图片存储目录必须填写绝对路径');

    draft.mediaRoot = '/mnt/archive/pixiv';
    await draft.save();
    expect(saveSetting).toHaveBeenCalledWith(
      'storage',
      {
        media_root: '/mnt/archive/pixiv',
        warning_threshold_bytes: 100 * 1024 ** 3,
        media_write_stop_threshold_bytes: 32 * 1024 ** 3,
        trash_retention_days: 30
      },
      2
    );
    expect(draft.message).toBe(
      '存储设置已经保存，图片目录将在Web和Worker重启后生效'
    );
  });

  it('keeps overview NSFW decorations exclusive with thumbnail masking', async () => {
    const saveSetting = vi
      .fn()
      .mockResolvedValue({ group: 'content', revision: 3 });
    const draft = new ContentSettingsDraft(
      {
        overview_allow_nsfw: true,
        mask_non_all_age_thumbnails: false
      },
      2,
      { saveSetting } as unknown as SystemApi
    );

    draft.setThumbnailMasking(true);
    expect(draft.overviewAllowNsfw).toBe(false);
    expect(draft.maskNonAllAgeThumbnails).toBe(true);
    expect(draft.dirty).toBe(true);

    draft.setThumbnailMasking(false);
    expect(draft.overviewAllowNsfw).toBe(false);
    await draft.save();

    expect(saveSetting).toHaveBeenCalledWith(
      'content',
      {
        overview_allow_nsfw: false,
        mask_non_all_age_thumbnails: false
      },
      2
    );
    expect(draft.dirty).toBe(false);
    expect(draft.message).toBe('非全年龄内容设置已经保存');
  });
});

function jobPriorities(): EffectiveSettings['queue']['job_priorities'] {
  return [
    { job_kind: 'scheduled_collection', priority: 'scheduled_collection' },
    { job_kind: 'ranking_collection', priority: 'scheduled_collection' },
    { job_kind: 'following_collection', priority: 'scheduled_collection' },
    { job_kind: 'bookmarks_collection', priority: 'scheduled_collection' },
    { job_kind: 'import_artist', priority: 'manual_import' },
    { job_kind: 'import_work', priority: 'manual_import' },
    { job_kind: 'download_media', priority: 'background_maintenance' },
    {
      job_kind: 'generate_derivative',
      priority: 'background_maintenance'
    },
    { job_kind: 'purge_trash', priority: 'background_maintenance' }
  ];
}
