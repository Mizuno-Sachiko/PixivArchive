import { endpoints } from './endpoints';

export type EventResource =
  | 'job'
  | 'rule'
  | 'work'
  | 'pixiv_bookmark'
  | 'pixiv_account'
  | 'deletion_marker'
  | 'subscription'
  | 'system_setting'
  | 'administrator';

export type AppInvalidation =
  | { kind: 'snapshot' }
  | { kind: 'resource'; resource: EventResource; resourceId: string };

interface EventStream {
  addEventListener(
    type: string,
    listener: (event: MessageEvent<string>) => void
  ): void;
  close(): void;
}

interface VisibilitySource {
  visibilityState: DocumentVisibilityState;
  addEventListener(type: 'visibilitychange', listener: () => void): void;
  removeEventListener(type: 'visibilitychange', listener: () => void): void;
}

export interface AppEventConnectionOptions {
  createEventSource?: (url: string) => EventStream;
  visibility?: VisibilitySource;
  onInvalidate: (invalidation: AppInvalidation) => void;
}

export interface AppEventConnection {
  close(): void;
}

interface AppEventMessage {
  resource: EventResource;
  resource_id: string;
}

export function connectAppEvents(
  options: AppEventConnectionOptions
): AppEventConnection {
  const createEventSource =
    options.createEventSource ?? createBrowserEventSource;
  const visibility = options.visibility ?? document;
  const source = createEventSource(endpoints.events);

  source.addEventListener('app_event', (event) => {
    const message = JSON.parse(event.data) as AppEventMessage;
    options.onInvalidate({
      kind: 'resource',
      resource: message.resource,
      resourceId: message.resource_id
    });
  });
  source.addEventListener('snapshot_refresh', () => {
    options.onInvalidate({ kind: 'snapshot' });
  });

  const refreshVisibleSnapshot = () => {
    if (visibility.visibilityState === 'visible') {
      options.onInvalidate({ kind: 'snapshot' });
    }
  };
  visibility.addEventListener('visibilitychange', refreshVisibleSnapshot);

  return {
    close() {
      visibility.removeEventListener(
        'visibilitychange',
        refreshVisibleSnapshot
      );
      source.close();
    }
  };
}

function createBrowserEventSource(url: string): EventStream {
  const source = new EventSource(url, { withCredentials: true });
  return {
    addEventListener(type, listener) {
      source.addEventListener(type, listener as EventListener);
    },
    close() {
      source.close();
    }
  };
}
