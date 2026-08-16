import {
  systemApi,
  type EffectiveSettings,
  type JobPriorityMapping,
  type SavedSetting,
  type SettingUpdate,
  type SystemApi
} from '$lib/api/system';

const gibibyte = 1024 ** 3;
const mebibyte = 1024 ** 2;

type SavedCallback = (saved: SavedSetting) => void;

abstract class SettingsDraft {
  busy = $state(false);
  message = $state('');
  error = $state('');

  protected constructor(
    protected readonly api: SystemApi = systemApi,
    private readonly onSaved?: SavedCallback
  ) {}

  protected accept(saved: SavedSetting): void {
    this.onSaved?.(saved);
  }

  protected async perform(
    label: string,
    operation: () => Promise<void>
  ): Promise<boolean> {
    if (this.busy) return false;
    this.busy = true;
    this.message = '';
    this.error = '';
    try {
      await operation();
      this.message = `${label}设置已经保存`;
      return true;
    } catch {
      this.error = `${label}设置保存失败`;
      return false;
    } finally {
      this.busy = false;
    }
  }
}

export class QueueSettingsDraft extends SettingsDraft {
  immediate: number;
  manualImport: number;
  scheduledCollection: number;
  backgroundMaintenance: number;
  private readonly jobPriorities: JobPriorityMapping[];

  constructor(
    original: EffectiveSettings['queue'],
    private revision: number | undefined,
    api: SystemApi = systemApi,
    onSaved?: SavedCallback
  ) {
    super(api, onSaved);
    this.immediate = $state(original.quota_weights.immediate);
    this.manualImport = $state(original.quota_weights.manual_import);
    this.scheduledCollection = $state(
      original.quota_weights.scheduled_collection
    );
    this.backgroundMaintenance = $state(
      original.quota_weights.background_maintenance
    );
    this.jobPriorities = original.job_priorities.map((mapping) => ({
      ...mapping
    }));
  }

  async save(): Promise<void> {
    await this.perform('队列', async () => {
      const saved = await this.api.saveSetting(
        'queue',
        {
          quota_weights: {
            immediate: this.immediate,
            manual_import: this.manualImport,
            scheduled_collection: this.scheduledCollection,
            background_maintenance: this.backgroundMaintenance
          },
          job_priorities: this.jobPriorities.map((mapping) => ({ ...mapping }))
        },
        this.revision
      );
      this.revision = saved.revision;
      this.accept(saved);
    });
  }
}

export class ProcessingSettingsDraft extends SettingsDraft {
  pixivConcurrency: number;
  pixivRate: number;
  downloadConcurrency: number;
  downloadRate: number;
  cpuConcurrency: number;

  constructor(
    value: NonNullable<EffectiveSettings['processing']>,
    private revision: number | undefined,
    api: SystemApi = systemApi,
    onSaved?: SavedCallback
  ) {
    super(api, onSaved);
    this.pixivConcurrency = $state(value.pixiv_request_concurrency);
    this.pixivRate = $state(value.pixiv_request_rate.requests);
    this.downloadConcurrency = $state(value.media_download_concurrency);
    this.downloadRate = $state(value.media_download_rate.requests);
    this.cpuConcurrency = $state(value.media_cpu_concurrency);
  }

  async save(): Promise<void> {
    await this.perform('处理限制', async () => {
      const saved = await this.api.saveSetting(
        'processing',
        {
          pixiv_request_concurrency: this.pixivConcurrency,
          pixiv_request_rate: { requests: this.pixivRate, per_seconds: 60 },
          media_download_concurrency: this.downloadConcurrency,
          media_download_rate: {
            requests: this.downloadRate,
            per_seconds: 60
          },
          media_cpu_concurrency: this.cpuConcurrency
        },
        this.revision
      );
      this.revision = saved.revision;
      this.accept(saved);
    });
  }
}

export class StorageSettingsDraft extends SettingsDraft {
  mediaRoot: string;
  warningGiB: number;
  writeStopGiB: number;
  trashDays: number;

  constructor(
    value: EffectiveSettings['storage'],
    private readonly activeMediaRoot: string,
    private revision: number | undefined,
    api: SystemApi = systemApi,
    onSaved?: SavedCallback
  ) {
    super(api, onSaved);
    this.mediaRoot = $state(value.media_root ?? activeMediaRoot);
    this.warningGiB = $state(
      Math.round(value.warning_threshold_bytes / gibibyte)
    );
    this.writeStopGiB = $state(
      Math.round(value.media_write_stop_threshold_bytes / gibibyte)
    );
    this.trashDays = $state(value.trash_retention_days);
  }

  async save(): Promise<void> {
    if (!this.mediaRoot.startsWith('/')) {
      this.message = '';
      this.error = '图片存储目录必须填写绝对路径';
      return;
    }

    const savedSuccessfully = await this.perform('存储', async () => {
      const saved = await this.api.saveSetting(
        'storage',
        {
          media_root: this.mediaRoot,
          warning_threshold_bytes: this.warningGiB * gibibyte,
          media_write_stop_threshold_bytes: this.writeStopGiB * gibibyte,
          trash_retention_days: this.trashDays
        },
        this.revision
      );
      this.revision = saved.revision;
      this.accept(saved);
    });
    if (savedSuccessfully && this.mediaRoot !== this.activeMediaRoot) {
      this.message = '存储设置已经保存，图片目录将在Web和Worker重启后生效';
    }
  }
}

export class CollectionSettingsDraft extends SettingsDraft {
  retryBackoff: string;
  derivativeFormat: 'webp' | 'avif';
  derivativeWidth: number;
  webpQuality: number;
  avifQuality: number;

  constructor(
    retry: EffectiveSettings['retry'],
    derivative: EffectiveSettings['derivative'],
    avifAvailable: boolean,
    private readonly revisions: Record<
      'retry' | 'derivative',
      number | undefined
    >,
    api: SystemApi = systemApi,
    onSaved?: SavedCallback
  ) {
    super(api, onSaved);
    this.retryBackoff = $state(retry.network_backoff_seconds.join(', '));
    this.derivativeFormat = $state(avifAvailable ? derivative.format : 'webp');
    this.derivativeWidth = $state(derivative.max_width);
    this.webpQuality = $state(derivative.webp_quality);
    this.avifQuality = $state(derivative.avif_quality);
  }

  async save(): Promise<void> {
    const backoff = parsePositiveIntegerList(this.retryBackoff);
    if (!backoff) {
      this.message = '';
      this.error = '网络错误退避秒数必须是递增的正整数列表';
      return;
    }

    await this.perform('采集默认值', async () => {
      const updates: SettingUpdate[] = [
        {
          group: 'retry',
          expected_revision: this.revisions.retry,
          value: { network_backoff_seconds: backoff }
        },
        {
          group: 'derivative',
          expected_revision: this.revisions.derivative,
          value: {
            format: this.derivativeFormat,
            max_width: this.derivativeWidth,
            webp_quality: this.webpQuality,
            avif_quality: this.avifQuality
          }
        }
      ];
      const saved = await this.api.saveSettings(updates);
      for (const setting of saved) {
        if (setting.group === 'retry' || setting.group === 'derivative') {
          this.revisions[setting.group] = setting.revision;
        }
        this.accept(setting);
      }
    });
  }
}

export class UgoiraSettingsDraft extends SettingsDraft {
  zipMiB: number;
  frames: number;
  pixels: number;
  cacheMiB: number;

  constructor(
    value: NonNullable<EffectiveSettings['ugoira']>,
    private revision: number | undefined,
    api: SystemApi = systemApi,
    onSaved?: SavedCallback
  ) {
    super(api, onSaved);
    this.zipMiB = $state(Math.round(value.max_zip_bytes / mebibyte));
    this.frames = $state(value.max_frames);
    this.pixels = $state(value.max_pixels_per_frame);
    this.cacheMiB = $state(
      Math.round(value.decoded_frame_cache_bytes / mebibyte)
    );
  }

  async save(): Promise<void> {
    await this.perform('动图', async () => {
      const saved = await this.api.saveSetting(
        'ugoira',
        {
          max_zip_bytes: this.zipMiB * mebibyte,
          max_frames: this.frames,
          max_pixels_per_frame: this.pixels,
          decoded_frame_cache_bytes: this.cacheMiB * mebibyte
        },
        this.revision
      );
      this.revision = saved.revision;
      this.accept(saved);
    });
  }
}

export class ContentSettingsDraft extends SettingsDraft {
  overviewAllowNsfw: boolean;
  maskNonAllAgeThumbnails: boolean;
  private savedOverviewAllowNsfw: boolean;
  private savedMaskNonAllAgeThumbnails: boolean;

  constructor(
    value: EffectiveSettings['content'],
    private revision: number | undefined,
    api: SystemApi = systemApi,
    onSaved?: SavedCallback
  ) {
    super(api, onSaved);
    this.overviewAllowNsfw = $state(value.overview_allow_nsfw);
    this.maskNonAllAgeThumbnails = $state(value.mask_non_all_age_thumbnails);
    this.savedOverviewAllowNsfw = value.overview_allow_nsfw;
    this.savedMaskNonAllAgeThumbnails = value.mask_non_all_age_thumbnails;
  }

  get dirty(): boolean {
    return (
      this.overviewAllowNsfw !== this.savedOverviewAllowNsfw ||
      this.maskNonAllAgeThumbnails !== this.savedMaskNonAllAgeThumbnails
    );
  }

  setThumbnailMasking(enabled: boolean): void {
    this.maskNonAllAgeThumbnails = enabled;
    if (enabled) this.overviewAllowNsfw = false;
  }

  value(): EffectiveSettings['content'] {
    return {
      overview_allow_nsfw: this.overviewAllowNsfw,
      mask_non_all_age_thumbnails: this.maskNonAllAgeThumbnails
    };
  }

  async save(): Promise<boolean> {
    const savedSuccessfully = await this.perform('非全年龄内容', async () => {
      const saved = await this.api.saveSetting(
        'content',
        this.value(),
        this.revision
      );
      this.revision = saved.revision;
      this.savedOverviewAllowNsfw = this.overviewAllowNsfw;
      this.savedMaskNonAllAgeThumbnails = this.maskNonAllAgeThumbnails;
      this.accept(saved);
    });
    return savedSuccessfully;
  }
}

export class PixivSettingsDraft extends SettingsDraft {
  defaultPrivateBookmark: boolean;

  constructor(
    value: EffectiveSettings['pixiv'],
    private revision: number | undefined,
    api: SystemApi = systemApi,
    onSaved?: SavedCallback
  ) {
    super(api, onSaved);
    this.defaultPrivateBookmark = $state(value.default_private_bookmark);
  }

  async save(): Promise<void> {
    await this.perform('Pixiv', async () => {
      const saved = await this.api.saveSetting(
        'pixiv',
        { default_private_bookmark: this.defaultPrivateBookmark },
        this.revision
      );
      this.revision = saved.revision;
      this.accept(saved);
    });
  }
}

function parsePositiveIntegerList(value: string): number[] | null {
  const values = value.split(',').map((item) => Number(item.trim()));
  if (
    values.length === 0 ||
    values.some((item) => !Number.isSafeInteger(item) || item <= 0) ||
    values.some((item, index) => index > 0 && values[index - 1] >= item)
  ) {
    return null;
  }
  return values;
}
