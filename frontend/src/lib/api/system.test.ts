import { describe, expect, it, vi } from 'vitest';

import type { ApiRequest } from './client';
import { createSystemApi, type SystemApi } from './system';

function verifySettingTypes(api: SystemApi): void {
  void api.saveSetting('pixiv', { default_private_bookmark: true });
  // @ts-expect-error A storage setting cannot receive a Pixiv payload.
  void api.saveSetting('storage', { default_private_bookmark: true });
}

void verifySettingTypes;

describe('system API', () => {
  it('sends only the Pixiv cookie when saving the account', async () => {
    const requestMock = vi.fn();
    const request: ApiRequest = async <T>(
      ...arguments_: Parameters<ApiRequest>
    ) => {
      requestMock(...arguments_);
      return {} as T;
    };
    const api = createSystemApi(request);

    await api.updateAccount({ cookie: '10001_session' });

    expect(requestMock).toHaveBeenCalledWith('/api/pixiv/account', {
      method: 'PUT',
      json: { cookie: '10001_session' }
    });
  });

  it('sends the hidden account revision when changing bookmark writeback', async () => {
    const requestMock = vi.fn();
    const request: ApiRequest = async <T>(
      ...arguments_: Parameters<ApiRequest>
    ) => {
      requestMock(...arguments_);
      return {} as T;
    };
    const api = createSystemApi(request);

    await api.setBookmarkWriteback(
      true,
      4,
      '0198f651-0000-7000-8000-000000000001'
    );

    const [, options] = requestMock.mock.calls[0];
    expect(options?.json).toEqual({
      expected_account_id: '0198f651-0000-7000-8000-000000000001',
      enabled: true,
      expected_revision: 4
    });
  });

  it('revalidates the saved Pixiv account without sending the cookie again', async () => {
    const requestMock = vi.fn();
    const request: ApiRequest = async <T>(
      ...arguments_: Parameters<ApiRequest>
    ) => {
      requestMock(...arguments_);
      return {} as T;
    };
    const api = createSystemApi(request);

    await api.validateAccount('0198f651-0000-7000-8000-000000000001');

    expect(requestMock).toHaveBeenCalledWith('/api/pixiv/account/validate', {
      method: 'POST',
      json: {
        expected_account_id: '0198f651-0000-7000-8000-000000000001'
      }
    });
  });

  it('clears the saved Pixiv credential with the current identity and revision', async () => {
    const requestMock = vi.fn();
    const request: ApiRequest = async <T>(
      ...arguments_: Parameters<ApiRequest>
    ) => {
      requestMock(...arguments_);
      return {} as T;
    };
    const api = createSystemApi(request);

    await api.clearAccountCredential('0198f651-0000-7000-8000-000000000001', 7);

    expect(requestMock).toHaveBeenCalledWith('/api/pixiv/account/credential', {
      method: 'DELETE',
      json: {
        expected_account_id: '0198f651-0000-7000-8000-000000000001',
        expected_revision: 7
      }
    });
  });

  it('saves the default private bookmark preference as a Pixiv setting', async () => {
    const requestMock = vi.fn();
    const request: ApiRequest = async <T>(
      ...arguments_: Parameters<ApiRequest>
    ) => {
      requestMock(...arguments_);
      return {} as T;
    };
    const api = createSystemApi(request);

    await api.saveSetting('pixiv', { default_private_bookmark: true }, 7);

    expect(requestMock).toHaveBeenCalledWith('/api/system/settings/pixiv', {
      method: 'PUT',
      json: {
        expected_revision: 7,
        value: { default_private_bookmark: true }
      }
    });
  });

  it('saves multiple setting groups in one request', async () => {
    const response = {
      settings: [
        { group: 'pixiv', revision: 3 },
        { group: 'retry', revision: 5 }
      ]
    };
    const requestMock = vi.fn();
    const request: ApiRequest = async <T>(
      ...arguments_: Parameters<ApiRequest>
    ) => {
      requestMock(...arguments_);
      return response as T;
    };
    const api = createSystemApi(request);

    const saved = await api.saveSettings([
      {
        group: 'pixiv',
        expected_revision: 2,
        value: { default_private_bookmark: true }
      },
      {
        group: 'retry',
        expected_revision: 4,
        value: { network_backoff_seconds: [60, 300] }
      }
    ]);

    expect(requestMock).toHaveBeenCalledWith('/api/system/settings', {
      method: 'PUT',
      json: {
        updates: [
          {
            group: 'pixiv',
            expected_revision: 2,
            value: { default_private_bookmark: true }
          },
          {
            group: 'retry',
            expected_revision: 4,
            value: { network_backoff_seconds: [60, 300] }
          }
        ]
      }
    });
    expect(saved).toEqual([
      { group: 'pixiv', revision: 3 },
      { group: 'retry', revision: 5 }
    ]);
  });
});
