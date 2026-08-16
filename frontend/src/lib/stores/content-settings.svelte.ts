import {
  systemApi,
  type EffectiveSettings,
  type SystemApi
} from '$lib/api/system';
import { LatestRequest } from '$lib/latest-request';

export type ContentSettings = EffectiveSettings['content'];

const unavailableSettings: ContentSettings = {
  overview_allow_nsfw: false,
  mask_non_all_age_thumbnails: true
};

class ContentSettingsStore {
  current = $state<ContentSettings | null>(null);
  loading = $state(false);
  error = $state('');
  private readonly requests = new LatestRequest();

  get effective(): ContentSettings {
    return this.current ?? unavailableSettings;
  }

  async load(api: SystemApi = systemApi): Promise<ContentSettings | null> {
    const request = this.requests.begin();
    this.loading = true;
    this.error = '';
    try {
      const settings = (await api.settings()).content;
      if (!this.requests.isCurrent(request)) return this.current;
      this.current = settings;
      return settings;
    } catch {
      if (this.requests.isCurrent(request)) {
        this.current = null;
        this.error = '内容显示设置暂时无法读取';
      }
      return this.current;
    } finally {
      if (this.requests.isCurrent(request)) this.loading = false;
    }
  }

  replace(settings: ContentSettings): void {
    this.requests.invalidate();
    this.current = { ...settings };
    this.error = '';
    this.loading = false;
  }

  reset(): void {
    this.requests.invalidate();
    this.current = null;
    this.error = '';
    this.loading = false;
  }
}

export const contentSettingsStore = new ContentSettingsStore();
