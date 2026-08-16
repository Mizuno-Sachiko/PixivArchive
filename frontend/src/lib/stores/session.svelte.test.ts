import { beforeEach, describe, expect, it } from 'vitest';

import type { ApiRequest } from '$lib/api/client';
import type { Session } from '$lib/api/types';

import { sessionStore } from './session.svelte';

const restoredSession: Session = {
  administrator_id: '0198f652-0000-7000-8000-000000000021',
  session_id: '0198f652-0000-7000-8000-000000000022',
  expires_at: '2026-08-03T12:00:00Z'
};
const authenticatedSession: Session = {
  ...restoredSession,
  session_id: '0198f652-0000-7000-8000-000000000023'
};

describe('session state', () => {
  beforeEach(() => {
    sessionStore.current = null;
    sessionStore.status = 'idle';
  });

  it('does not let a slow restore replace a newer sign in', async () => {
    const restoreRequest = deferred<Session>();
    const restoring = sessionStore.restore(
      requestReturning(restoreRequest.promise)
    );
    await sessionStore.signIn(
      { password: 'current-password' },
      requestReturning(Promise.resolve(authenticatedSession))
    );

    restoreRequest.resolve(restoredSession);
    expect(await restoring).toEqual(authenticatedSession);
    expect(sessionStore.current).toEqual(authenticatedSession);
    expect(sessionStore.status).toBe('authenticated');
  });

  it('does not let a slow sign out clear a newer sign in', async () => {
    sessionStore.current = restoredSession;
    sessionStore.status = 'authenticated';
    const signOutRequest = deferred<void>();
    const signingOut = sessionStore.signOut(
      requestReturning(signOutRequest.promise)
    );
    await sessionStore.signIn(
      { password: 'current-password' },
      requestReturning(Promise.resolve(authenticatedSession))
    );

    signOutRequest.resolve();
    await signingOut;
    expect(sessionStore.current).toEqual(authenticatedSession);
    expect(sessionStore.status).toBe('authenticated');
  });
});

function requestReturning<T>(response: Promise<T>): ApiRequest {
  return (() => response) as ApiRequest;
}

function deferred<T>(): {
  promise: Promise<T>;
  resolve: (value: T) => void;
} {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((next) => {
    resolve = next;
  });
  return { promise, resolve };
}
