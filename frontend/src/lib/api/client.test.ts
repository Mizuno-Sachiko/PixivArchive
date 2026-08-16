import { describe, expect, it, vi } from 'vitest';

import { ApiError, ConflictError, createApiRequest } from './client';
import { connectAppEvents, type AppInvalidation } from './events';

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json' }
  });
}

describe('apiRequest', () => {
  it('uses same-origin credentials without sending CSRF on reads', async () => {
    const fetchMock = vi.fn<typeof fetch>();
    fetchMock.mockResolvedValue(jsonResponse({ ready: true }));
    const request = createApiRequest({
      fetch: fetchMock,
      csrfToken: () => 'csrf-token'
    });

    await request<{ ready: boolean }>('/api/system/status');

    const [, init] = fetchMock.mock.calls[0];
    expect(init?.credentials).toBe('same-origin');
    expect(new Headers(init?.headers).has('X-CSRF-Token')).toBe(false);
  });

  it('adds JSON and CSRF headers to mutating requests', async () => {
    const fetchMock = vi.fn<typeof fetch>();
    fetchMock.mockResolvedValue(new Response(null, { status: 204 }));
    const request = createApiRequest({
      fetch: fetchMock,
      csrfToken: () => 'csrf-token'
    });

    await request<void>('/api/system/maintenance', {
      method: 'POST',
      json: { operation: 'scan_expired_trash' }
    });

    const [, init] = fetchMock.mock.calls[0];
    const headers = new Headers(init?.headers);
    expect(init?.credentials).toBe('same-origin');
    expect(headers.get('Content-Type')).toBe('application/json');
    expect(headers.get('X-CSRF-Token')).toBe('csrf-token');
    expect(init?.body).toBe(
      JSON.stringify({ operation: 'scan_expired_trash' })
    );
  });

  it('parses API errors and keeps the trace ID available', async () => {
    const fetchMock = vi.fn<typeof fetch>();
    fetchMock.mockResolvedValue(
      jsonResponse(
        {
          code: 'invalid_request',
          message: 'The request is invalid',
          details: { field: 'name' },
          trace_id: '0198f63e-f063-778b-aac4-99bf9959ec68'
        },
        422
      )
    );
    const request = createApiRequest({
      fetch: fetchMock,
      csrfToken: () => undefined
    });

    const failure = request('/api/rules', {
      method: 'POST',
      json: { name: '' }
    });

    await expect(failure).rejects.toMatchObject({
      status: 422,
      code: 'invalid_request',
      details: { field: 'name' },
      traceId: '0198f63e-f063-778b-aac4-99bf9959ec68'
    });
    await expect(failure).rejects.toBeInstanceOf(ApiError);
  });

  it('uses a distinct conflict error for revision conflicts', async () => {
    const fetchMock = vi.fn<typeof fetch>();
    fetchMock.mockResolvedValue(
      jsonResponse(
        {
          code: 'revision_conflict',
          message: 'The resource changed after it was loaded',
          details: { current_revision: 8 },
          trace_id: '0198f63f-6577-7a82-8824-f3ec417184d4'
        },
        409
      )
    );
    const request = createApiRequest({
      fetch: fetchMock,
      csrfToken: () => 'csrf-token'
    });

    const failure = request('/api/rules/example/draft', {
      method: 'PUT',
      json: { expected_revision: 7 }
    });

    await expect(failure).rejects.toBeInstanceOf(ConflictError);
    await expect(failure).rejects.toMatchObject({
      status: 409,
      code: 'revision_conflict',
      details: { current_revision: 8 }
    });
  });
});

describe('app events', () => {
  it('invalidates the full snapshot when a hidden page becomes visible', () => {
    const source = new FakeEventSource();
    const visibility = new FakeVisibility();
    const invalidations: AppInvalidation[] = [];
    const connection = connectAppEvents({
      createEventSource: () => source,
      visibility,
      onInvalidate: (invalidation) => invalidations.push(invalidation)
    });

    visibility.show();

    expect(invalidations).toEqual([{ kind: 'snapshot' }]);
    connection.close();
    expect(source.closed).toBe(true);
  });

  it('maps server events to resource and snapshot invalidations', () => {
    const source = new FakeEventSource();
    const invalidations: AppInvalidation[] = [];
    const connection = connectAppEvents({
      createEventSource: () => source,
      visibility: new FakeVisibility(),
      onInvalidate: (invalidation) => invalidations.push(invalidation)
    });

    source.emit(
      'app_event',
      JSON.stringify({
        id: 42,
        resource: 'work',
        resource_id: '0198f641-a544-7437-a04b-b4439b5c20c0',
        payload: { type: 'work_changed', revision: 3 }
      })
    );
    source.emit('snapshot_refresh', JSON.stringify({ latest_event_id: 42 }));

    expect(invalidations).toEqual([
      {
        kind: 'resource',
        resource: 'work',
        resourceId: '0198f641-a544-7437-a04b-b4439b5c20c0'
      },
      { kind: 'snapshot' }
    ]);
    connection.close();
  });
});

type EventListener = (event: MessageEvent<string>) => void;

class FakeEventSource {
  closed = false;
  private readonly listeners = new Map<string, Set<EventListener>>();

  addEventListener(type: string, listener: EventListener): void {
    const listeners = this.listeners.get(type) ?? new Set<EventListener>();
    listeners.add(listener);
    this.listeners.set(type, listeners);
  }

  close(): void {
    this.closed = true;
  }

  emit(type: string, data: string): void {
    const event = new MessageEvent(type, { data });
    for (const listener of this.listeners.get(type) ?? []) {
      listener(event);
    }
  }
}

class FakeVisibility {
  visibilityState: DocumentVisibilityState = 'hidden';
  private readonly listeners = new Set<() => void>();

  addEventListener(_type: 'visibilitychange', listener: () => void): void {
    this.listeners.add(listener);
  }

  removeEventListener(_type: 'visibilitychange', listener: () => void): void {
    this.listeners.delete(listener);
  }

  show(): void {
    this.visibilityState = 'visible';
    for (const listener of this.listeners) {
      listener();
    }
  }
}
