import { describe, expect, it, vi } from 'vitest';

import type { ApiRequest } from './client';
import { createSubscriptionApi } from './subscriptions';

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

describe('subscription API', () => {
  it('uses the shared enabled command for every subscription kind', async () => {
    const { calls, request } = recordingRequest();
    const api = createSubscriptionApi(request);

    await api.setEnabled('subscription-id', 7, false);

    expect(calls).toHaveBeenCalledWith(
      '/api/subscriptions/subscription-id/enabled',
      {
        method: 'PUT',
        json: { expected_revision: 7, enabled: false }
      }
    );
  });

  it('stops the current run without disabling its subscription', async () => {
    const { calls, request } = recordingRequest();
    const api = createSubscriptionApi(request);

    await api.stop('subscription-id');

    expect(calls).toHaveBeenCalledWith(
      '/api/subscriptions/subscription-id/stop',
      { method: 'POST' }
    );
  });
});
