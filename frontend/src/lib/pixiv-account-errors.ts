import { ApiError, ConflictError } from '$lib/api/client';
import type { AccountState } from '$lib/api/system';

interface ValidationResult {
  message: string;
  error: boolean;
}

export function pixivAccountFailureMessage(error: unknown): string {
  if (!(error instanceof ApiError)) return 'Pixiv账户验证请求失败';
  if (error.code === 'invalid_request') {
    return 'Cookie格式不正确，请填写PHPSESSID或完整Cookie';
  }
  if (error.code === 'pixiv_credential_invalid') {
    return 'Pixiv Cookie已经失效，或Cookie中的账户身份不一致';
  }
  if (error.code === 'pixiv_rate_limited') {
    return 'Pixiv暂时限制了验证请求，请稍后重试';
  }
  if (error.code !== 'pixiv_validation_unavailable') {
    return 'Pixiv账户验证请求失败';
  }

  const errorClass = validationErrorClass(error.details);
  if (errorClass === 'network') {
    return '当前无法连接Pixiv，请检查网络或代理设置';
  }
  if (errorClass === 'invalid_json_or_interstitial') {
    return 'Pixiv返回了验证页面，当前请求未能完成';
  }
  if (errorClass === 'temporary_pixiv_error') {
    return 'Pixiv服务暂时异常，请稍后重试';
  }
  return 'Pixiv账户验证暂时无法完成';
}

export function pixivAccountActionFailureMessage(
  error: unknown,
  fallback: string
): string {
  if (error instanceof ConflictError) {
    return '当前Pixiv账户或页面数据已经变化，请重新读取后再试';
  }
  return fallback;
}

export function isPixivAccountConflict(error: unknown): boolean {
  return error instanceof ConflictError;
}

export function pixivAccountValidationResult(
  state: AccountState
): ValidationResult {
  switch (state) {
    case 'unconfigured':
      return {
        message: '尚未配置Pixiv账户，请先保存Cookie',
        error: true
      };
    case 'validating':
      return {
        message: 'Pixiv账户正在验证，请稍后查看',
        error: true
      };
    case 'normal':
      return { message: '验证成功', error: false };
    case 'credential_invalid':
      return {
        message: 'Pixiv Cookie已经失效，请重新填写',
        error: true
      };
    case 'restricted':
      return {
        message: 'Cookie有效，但当前Pixiv账户无法访问R-18内容',
        error: false
      };
  }
}

function validationErrorClass(details: unknown): string | null {
  if (!details || typeof details !== 'object') return null;
  const value = (details as Record<string, unknown>).error_class;
  return typeof value === 'string' ? value : null;
}
