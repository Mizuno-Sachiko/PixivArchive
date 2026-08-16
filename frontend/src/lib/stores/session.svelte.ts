import {
  ApiError,
  type ApiRequest,
  apiRequest as defaultApiRequest
} from '$lib/api/client';
import { endpoints } from '$lib/api/endpoints';
import type { LoginBody, Session } from '$lib/api/types';
import { LatestRequest } from '$lib/latest-request';

export type SessionStatus = 'idle' | 'loading' | 'authenticated' | 'anonymous';

class SessionStore {
  current = $state<Session | null>(null);
  status = $state<SessionStatus>('idle');
  private readonly operations = new LatestRequest();

  async restore(
    request: ApiRequest = defaultApiRequest
  ): Promise<Session | null> {
    if (this.current) {
      this.status = 'authenticated';
      return this.current;
    }
    const operation = this.operations.begin();
    this.status = 'loading';
    try {
      const restored = await request<Session>(endpoints.session);
      if (!this.operations.isCurrent(operation)) return this.current;
      this.current = restored;
      this.status = 'authenticated';
      return this.current;
    } catch (error) {
      if (!this.operations.isCurrent(operation)) return this.current;
      if (error instanceof ApiError && error.status === 401) {
        this.current = null;
        this.status = 'anonymous';
        return null;
      }
      this.status = 'anonymous';
      throw error;
    }
  }

  async signIn(
    credentials: LoginBody,
    request: ApiRequest = defaultApiRequest
  ): Promise<Session> {
    const operation = this.operations.begin();
    this.status = 'loading';
    try {
      const authenticated = await request<Session>(endpoints.login, {
        method: 'POST',
        json: credentials
      });
      if (!this.operations.isCurrent(operation)) {
        throw new Error('session operation was superseded');
      }
      this.current = authenticated;
      this.status = 'authenticated';
      return this.current;
    } catch (error) {
      if (!this.operations.isCurrent(operation)) throw error;
      this.current = null;
      this.status = 'anonymous';
      throw error;
    }
  }

  async signOut(request: ApiRequest = defaultApiRequest): Promise<void> {
    const operation = this.operations.begin();
    this.status = 'loading';
    try {
      await request<void>(endpoints.logout, { method: 'POST' });
      if (!this.operations.isCurrent(operation)) return;
      this.current = null;
      this.status = 'anonymous';
    } catch (error) {
      if (this.operations.isCurrent(operation)) {
        this.status = this.current ? 'authenticated' : 'anonymous';
      }
      throw error;
    }
  }
}

export const sessionStore = new SessionStore();
