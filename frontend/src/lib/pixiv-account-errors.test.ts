import { describe, expect, it } from 'vitest';

import { ApiError, ConflictError } from '$lib/api/client';
import {
  pixivAccountActionFailureMessage,
  pixivAccountFailureMessage,
  pixivAccountValidationResult
} from './pixiv-account-errors';

describe('Pixiv account validation messages', () => {
  it('explains an expired credential', () => {
    const error = new ApiError(422, {
      code: 'pixiv_credential_invalid',
      message: 'invalid',
      details: { error_class: 'credential_invalid', endpoint: 'profile' },
      trace_id: 'trace'
    });

    expect(pixivAccountFailureMessage(error)).toContain('失效');
  });

  it('distinguishes an interstitial response from a network failure', () => {
    const error = new ApiError(503, {
      code: 'pixiv_validation_unavailable',
      message: 'unavailable',
      details: {
        error_class: 'invalid_json_or_interstitial',
        endpoint: 'profile'
      },
      trace_id: 'trace'
    });

    expect(pixivAccountFailureMessage(error)).toContain('验证页面');
  });

  it('reports a saved invalid account as an error result', () => {
    expect(pixivAccountValidationResult('credential_invalid')).toEqual({
      message: 'Pixiv Cookie已经失效，请重新填写',
      error: true
    });
  });

  it('explains unconfigured and in-progress validation states separately', () => {
    expect(pixivAccountValidationResult('unconfigured')).toEqual({
      message: '尚未配置Pixiv账户，请先保存Cookie',
      error: true
    });
    expect(pixivAccountValidationResult('validating')).toEqual({
      message: 'Pixiv账户正在验证，请稍后查看',
      error: true
    });
  });

  it('explains that an account-bound action used stale page data', () => {
    const error = new ConflictError({
      code: 'revision_conflict',
      message: 'conflict',
      details: {},
      trace_id: 'trace'
    });

    expect(pixivAccountActionFailureMessage(error, 'Pixiv操作失败')).toBe(
      '当前Pixiv账户或页面数据已经变化，请重新读取后再试'
    );
  });
});
