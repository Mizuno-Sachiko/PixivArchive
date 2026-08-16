import { apiRequest, type ApiRequest } from './client';
import type { components } from './schema';
import type { Subscription, SubscriptionRunAccepted } from './subscriptions';

export type FollowingAuthor = components['schemas']['FollowingAuthorDto'];
export type FollowingState = components['schemas']['FollowingStateDto'];
export type ArtistFollowState = components['schemas']['ArtistFollowStateDto'];

export interface FollowingApi {
  get(): Promise<FollowingState>;
  updateSubscription(
    enabled: boolean,
    intervalMinutes: number,
    expectedRevision: number,
    expectedAccountId: string
  ): Promise<Subscription>;
  updateAuthor(
    pixivArtistId: number,
    enabled: boolean,
    expectedAccountId: string
  ): Promise<FollowingAuthor>;
  updateAuthors(
    pixivArtistIds: number[],
    enabled: boolean,
    expectedAccountId: string
  ): Promise<FollowingState>;
  artistFollowState(
    pixivArtistId: number,
    expectedAccountId: string
  ): Promise<ArtistFollowState>;
  updateArtistFollow(
    pixivArtistId: number,
    followed: boolean,
    expectedAccountId: string
  ): Promise<ArtistFollowState>;
  run(
    expectedAccountId: string,
    backfill?: boolean
  ): Promise<SubscriptionRunAccepted>;
  refresh(expectedAccountId: string): Promise<FollowingState>;
}

export function createFollowingApi(
  request: ApiRequest = apiRequest
): FollowingApi {
  return {
    get() {
      return request('/api/following');
    },
    updateSubscription(
      enabled,
      intervalMinutes,
      expectedRevision,
      expectedAccountId
    ) {
      return request('/api/following', {
        method: 'PUT',
        json: {
          expected_account_id: expectedAccountId,
          enabled,
          interval_minutes: intervalMinutes,
          expected_revision: expectedRevision
        }
      });
    },
    updateAuthor(pixivArtistId, enabled, expectedAccountId) {
      return request(`/api/following/authors/${pixivArtistId}`, {
        method: 'PUT',
        json: { expected_account_id: expectedAccountId, enabled }
      });
    },
    updateAuthors(pixivArtistIds, enabled, expectedAccountId) {
      return request('/api/following/authors', {
        method: 'PUT',
        json: {
          expected_account_id: expectedAccountId,
          pixiv_artist_ids: pixivArtistIds,
          enabled
        }
      });
    },
    artistFollowState(pixivArtistId, expectedAccountId) {
      return request(
        `/api/following/authors/${pixivArtistId}/pixiv?expected_account_id=${encodeURIComponent(expectedAccountId)}`
      );
    },
    updateArtistFollow(pixivArtistId, followed, expectedAccountId) {
      return request(`/api/following/authors/${pixivArtistId}/pixiv`, {
        method: 'PUT',
        json: { expected_account_id: expectedAccountId, followed }
      });
    },
    run(expectedAccountId, backfill = false) {
      return request('/api/following/run', {
        method: 'POST',
        json: { expected_account_id: expectedAccountId, backfill }
      });
    },
    refresh(expectedAccountId) {
      return request('/api/following/refresh', {
        method: 'POST',
        json: { expected_account_id: expectedAccountId }
      });
    }
  };
}

export const followingApi = createFollowingApi();

export function followingAuthorAvatarUrl(pixivArtistId: number): string {
  return `/api/following/authors/${pixivArtistId}/avatar`;
}
