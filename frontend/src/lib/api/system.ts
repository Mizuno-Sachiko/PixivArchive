import { apiRequest, type ApiRequest } from './client';
import { endpoints } from './endpoints';
import type { components } from './schema';

export type AccountState = components['schemas']['PixivAccountStateDto'];
export type PixivAccount = components['schemas']['PixivAccountDto'];
export type PixivAccountUpdate =
  components['schemas']['UpdatePixivAccountBody'];
export type ComponentStatus = components['schemas']['ComponentStatusDto'];
export type StorageStatus = components['schemas']['StorageStatusDto'];
export type DeploymentCapabilities =
  components['schemas']['SystemCapabilitiesDto'];
export type SystemStatus = components['schemas']['SystemStatusDto'];
export type FailureLimit = components['schemas']['FailureLimitDto'];
export type JobPriority = components['schemas']['JobPriorityDto'];
export type JobKind = components['schemas']['JobKindDto'];
export type JobPriorityMapping = components['schemas']['JobPriorityMappingDto'];
export type EffectiveSettings = components['schemas']['EffectiveSettingsDto'];

export type SettingGroup = keyof EffectiveSettings;

export type SettingUpdate = {
  [Group in SettingGroup]: {
    group: Group;
    value: NonNullable<EffectiveSettings[Group]>;
    expected_revision?: number;
  };
}[SettingGroup];

type SaveSettingArguments = {
  [Group in SettingGroup]: [
    group: Group,
    value: NonNullable<EffectiveSettings[Group]>,
    expectedRevision?: number
  ];
}[SettingGroup];

export type SavedSetting = components['schemas']['SavedSettingDto'];
export type MaintenanceAccepted =
  components['schemas']['MaintenanceAcceptedDto'];

export interface SystemApi {
  status(): Promise<SystemStatus>;
  settings(): Promise<EffectiveSettings>;
  saveSetting(...arguments_: SaveSettingArguments): Promise<SavedSetting>;
  saveSettings(updates: SettingUpdate[]): Promise<SavedSetting[]>;
  account(): Promise<PixivAccount>;
  updateAccount(input: PixivAccountUpdate): Promise<PixivAccount>;
  validateAccount(expectedAccountId: string): Promise<PixivAccount>;
  clearAccountCredential(
    expectedAccountId: string,
    expectedRevision: number
  ): Promise<PixivAccount>;
  setBookmarkWriteback(
    enabled: boolean,
    expectedRevision: number,
    expectedAccountId: string
  ): Promise<PixivAccount>;
  maintenance(operation: string): Promise<MaintenanceAccepted>;
}

export function createSystemApi(request: ApiRequest = apiRequest): SystemApi {
  return {
    status() {
      return request(endpoints.systemStatus);
    },
    async settings() {
      const response = await request<components['schemas']['SettingsDto']>(
        endpoints.systemSettings
      );
      return response.value;
    },
    saveSetting(...[group, value, expectedRevision]) {
      return request(`/api/system/settings/${group}`, {
        method: 'PUT',
        json: {
          expected_revision: expectedRevision ?? null,
          value
        }
      });
    },
    async saveSettings(updates) {
      const response = await request<components['schemas']['SavedSettingsDto']>(
        endpoints.systemSettings,
        {
          method: 'PUT',
          json: {
            updates: updates.map((update) => ({
              ...update,
              expected_revision: update.expected_revision ?? null
            }))
          }
        }
      );
      return response.settings;
    },
    account() {
      return request(endpoints.pixivAccount);
    },
    updateAccount(input) {
      return request(endpoints.pixivAccount, { method: 'PUT', json: input });
    },
    validateAccount(expectedAccountId) {
      return request('/api/pixiv/account/validate', {
        method: 'POST',
        json: { expected_account_id: expectedAccountId }
      });
    },
    clearAccountCredential(expectedAccountId, expectedRevision) {
      return request('/api/pixiv/account/credential', {
        method: 'DELETE',
        json: {
          expected_account_id: expectedAccountId,
          expected_revision: expectedRevision
        }
      });
    },
    setBookmarkWriteback(enabled, expectedRevision, expectedAccountId) {
      return request('/api/pixiv/account/bookmark-writeback', {
        method: 'PUT',
        json: {
          expected_account_id: expectedAccountId,
          enabled,
          expected_revision: expectedRevision
        }
      });
    },
    maintenance(operation) {
      return request('/api/system/maintenance', {
        method: 'POST',
        json: { operation }
      });
    }
  };
}

export const systemApi = createSystemApi();
