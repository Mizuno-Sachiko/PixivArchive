import { apiRequest, type ApiRequest } from './client';
import type { components } from './schema';
import type { SubscriptionRunAccepted } from './subscriptions';

export type FavoritesState = components['schemas']['FavoritesStateDto'];

export interface FavoritesApi {
  get(): Promise<FavoritesState>;
  update(
    enabled: boolean,
    intervalMinutes: number,
    expectedRevision: number,
    expectedAccountId: string
  ): Promise<FavoritesState>;
  run(expectedAccountId: string): Promise<SubscriptionRunAccepted>;
}

export function createFavoritesApi(
  request: ApiRequest = apiRequest
): FavoritesApi {
  return {
    get() {
      return request('/api/favorites');
    },
    update(enabled, intervalMinutes, expectedRevision, expectedAccountId) {
      return request('/api/favorites', {
        method: 'PUT',
        json: {
          expected_account_id: expectedAccountId,
          enabled,
          interval_minutes: intervalMinutes,
          expected_revision: expectedRevision
        }
      });
    },
    run(expectedAccountId) {
      return request('/api/favorites/run', {
        method: 'POST',
        json: { expected_account_id: expectedAccountId }
      });
    }
  };
}

export const favoritesApi = createFavoritesApi();
