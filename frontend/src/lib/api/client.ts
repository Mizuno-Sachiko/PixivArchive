import type { ApiErrorBody } from './types';

const MUTATING_METHODS = new Set(['POST', 'PUT', 'PATCH', 'DELETE']);
const CSRF_COOKIE = 'pa_csrf';

export interface ApiRequestOptions extends Omit<RequestInit, 'body'> {
  json?: unknown;
}

export interface ApiRequestDependencies {
  fetch: typeof fetch;
  csrfToken: () => string | undefined;
}

export type ApiRequest = <T>(
  path: string,
  options?: ApiRequestOptions
) => Promise<T>;

export class ApiError extends Error {
  readonly status: number;
  readonly code: string;
  readonly details: unknown;
  readonly traceId: string | null;

  constructor(status: number, body: ApiErrorBody) {
    super(body.message);
    this.name = 'ApiError';
    this.status = status;
    this.code = body.code;
    this.details = body.details;
    this.traceId = body.trace_id;
  }
}

export class ConflictError extends ApiError {
  constructor(body: ApiErrorBody) {
    super(409, body);
    this.name = 'ConflictError';
  }
}

export function createApiRequest(
  dependencies: ApiRequestDependencies
): ApiRequest {
  return async <T>(
    path: string,
    options: ApiRequestOptions = {}
  ): Promise<T> => {
    const {
      json,
      headers: initialHeaders,
      method = 'GET',
      ...requestOptions
    } = options;
    const normalizedMethod = method.toUpperCase();
    const headers = new Headers(initialHeaders);
    let body: string | undefined;

    if (json !== undefined) {
      headers.set('Content-Type', 'application/json');
      body = JSON.stringify(json);
    }

    if (MUTATING_METHODS.has(normalizedMethod)) {
      const token = dependencies.csrfToken();
      if (token) {
        headers.set('X-CSRF-Token', token);
      }
    }

    const response = await dependencies.fetch(path, {
      ...requestOptions,
      method: normalizedMethod,
      headers,
      body,
      credentials: 'same-origin'
    });

    if (!response.ok) {
      throw await responseError(response);
    }
    if (response.status === 204) {
      return undefined as T;
    }
    return (await response.json()) as T;
  };
}

export const apiRequest = createApiRequest({
  fetch: (...arguments_) => globalThis.fetch(...arguments_),
  csrfToken: browserCsrfToken
});

function browserCsrfToken(): string | undefined {
  if (typeof document === 'undefined') {
    return undefined;
  }
  return cookieValue(document.cookie, CSRF_COOKIE);
}

function cookieValue(cookieHeader: string, name: string): string | undefined {
  for (const entry of cookieHeader.split(';')) {
    const [key, ...valueParts] = entry.trim().split('=');
    if (key === name) {
      return valueParts.join('=');
    }
  }
  return undefined;
}

async function responseError(response: Response): Promise<ApiError> {
  const body = await readErrorBody(response);
  return response.status === 409
    ? new ConflictError(body)
    : new ApiError(response.status, body);
}

async function readErrorBody(response: Response): Promise<ApiErrorBody> {
  if (response.headers.get('Content-Type')?.includes('application/json')) {
    const body = (await response.json()) as ApiErrorBody;
    return body;
  }
  return {
    code: `http_${response.status}`,
    message: response.statusText || 'The request failed',
    details: {},
    trace_id: ''
  };
}
