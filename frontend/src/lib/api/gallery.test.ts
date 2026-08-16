import { describe, expect, it, vi } from 'vitest';

import { ApiError, type ApiRequest } from './client';
import { resolveWorkIdByPixivId } from './gallery';

describe('gallery API', () => {
  it('resolves a Pixiv work ID across collected and trashed works', async () => {
    const requestMock = vi.fn();
    const request: ApiRequest = async <T>(
      ...arguments_: Parameters<ApiRequest>
    ) => {
      requestMock(...arguments_);
      return {
        work_id: '0198f64c-42a2-7374-bace-9f1c3b317fb0'
      } as T;
    };

    await expect(resolveWorkIdByPixivId(1002, request)).resolves.toBe(
      '0198f64c-42a2-7374-bace-9f1c3b317fb0'
    );

    expect(requestMock).toHaveBeenCalledWith('/api/works/by-pixiv-id/1002');
  });

  it('maps a missing Pixiv work ID to null without hiding other failures', async () => {
    const missing: ApiRequest = async () => {
      throw new ApiError(404, {
        code: 'not_found',
        message: 'missing',
        details: {},
        trace_id: '0198f64c-42a2-7374-bace-9f1c3b317fb0'
      });
    };
    const unavailable = new ApiError(503, {
      code: 'service_unavailable',
      message: 'unavailable',
      details: {},
      trace_id: '0198f64c-42a2-7374-bace-9f1c3b317fb0'
    });
    const failed: ApiRequest = async () => {
      throw unavailable;
    };

    await expect(resolveWorkIdByPixivId(1002, missing)).resolves.toBeNull();
    await expect(resolveWorkIdByPixivId(1002, failed)).rejects.toBe(
      unavailable
    );
  });
});
