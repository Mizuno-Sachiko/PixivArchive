import { describe, expect, it, vi } from 'vitest';

import type { ApiRequest } from './client';
import { createFavoritesApi } from './favorites';

describe('favorites API', () => {
  it('sends the expected Pixiv account with every mutating command', async () => {
    const calls = vi.fn();
    const request: ApiRequest = async <T>(
      ...arguments_: Parameters<ApiRequest>
    ) => {
      calls(...arguments_);
      return {} as T;
    };
    const api = createFavoritesApi(request);
    const accountId = '019fe98e-0e22-7000-8000-000000000001';

    await api.update(true, 30, 4, accountId);
    await api.run(accountId);

    expect(calls.mock.calls).toEqual([
      [
        '/api/favorites',
        {
          method: 'PUT',
          json: {
            expected_account_id: accountId,
            enabled: true,
            interval_minutes: 30,
            expected_revision: 4
          }
        }
      ],
      [
        '/api/favorites/run',
        { method: 'POST', json: { expected_account_id: accountId } }
      ]
    ]);
  });
});
