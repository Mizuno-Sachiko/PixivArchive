import type { EventResource } from '$lib/api/events';
import { ResourceRefreshCoordinator } from '$lib/resource-refresh';
import { appEventsStore } from '$lib/stores/app-events.svelte';

export function composeAppEventVersion(
  snapshotRevision: number,
  resourceRevisions: Readonly<Record<EventResource, number>>,
  resources: readonly EventResource[]
): string {
  return [
    snapshotRevision,
    ...resources.map((resource) => resourceRevisions[resource])
  ].join(':');
}

export function currentAppEventVersion(
  resources: readonly EventResource[]
): string {
  return composeAppEventVersion(
    appEventsStore.snapshotRevision,
    appEventsStore.resourceRevisions,
    resources
  );
}

export class AppEventRefreshCoordinator extends ResourceRefreshCoordinator<string> {
  constructor(refresh: () => Promise<boolean>) {
    super(null, refresh, { sameVersion: (left, right) => left === right });
  }
}
