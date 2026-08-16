import { describe, expect, it, vi } from 'vitest';

import type { ApiRequest } from './client';
import { createFollowingApi, followingAuthorAvatarUrl } from './following';

function recordingRequest() {
  const calls = vi.fn();
  const request: ApiRequest = async <T>(
    ...arguments_: Parameters<ApiRequest>
  ) => {
    calls(...arguments_);
    return {} as T;
  };
  return { calls, request };
}

describe('following API', () => {
  it('owns the authenticated author avatar URL', () => {
    expect(followingAuthorAvatarUrl(70001)).toBe(
      '/api/following/authors/70001/avatar'
    );
  });

  it('sends the expected Pixiv account with every account-bound command', async () => {
    const { calls, request } = recordingRequest();
    const api = createFollowingApi(request);
    const accountId = '019fe98e-0e22-7000-8000-000000000001';

    await api.updateSubscription(false, 15, 4, accountId);
    await api.updateAuthor(70001, false, accountId);
    await api.updateAuthors([70001, 70002], true, accountId);
    await api.artistFollowState(70001, accountId);
    await api.updateArtistFollow(70001, false, accountId);
    await api.run(accountId);
    await api.run(accountId, true);
    await api.refresh(accountId);

    expect(calls.mock.calls).toEqual([
      [
        '/api/following',
        {
          method: 'PUT',
          json: {
            expected_account_id: accountId,
            enabled: false,
            interval_minutes: 15,
            expected_revision: 4
          }
        }
      ],
      [
        '/api/following/authors/70001',
        {
          method: 'PUT',
          json: { expected_account_id: accountId, enabled: false }
        }
      ],
      [
        '/api/following/authors',
        {
          method: 'PUT',
          json: {
            expected_account_id: accountId,
            pixiv_artist_ids: [70001, 70002],
            enabled: true
          }
        }
      ],
      [`/api/following/authors/70001/pixiv?expected_account_id=${accountId}`],
      [
        '/api/following/authors/70001/pixiv',
        {
          method: 'PUT',
          json: { expected_account_id: accountId, followed: false }
        }
      ],
      [
        '/api/following/run',
        {
          method: 'POST',
          json: { expected_account_id: accountId, backfill: false }
        }
      ],
      [
        '/api/following/run',
        {
          method: 'POST',
          json: { expected_account_id: accountId, backfill: true }
        }
      ],
      [
        '/api/following/refresh',
        { method: 'POST', json: { expected_account_id: accountId } }
      ]
    ]);
  });
});
