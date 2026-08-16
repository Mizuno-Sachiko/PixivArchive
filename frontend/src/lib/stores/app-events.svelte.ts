import {
  connectAppEvents,
  type AppEventConnection,
  type AppInvalidation,
  type EventResource
} from '$lib/api/events';

class AppEventsStore {
  snapshotRevision = $state(0);
  resourceRevisions = $state<Record<EventResource, number>>({
    job: 0,
    rule: 0,
    work: 0,
    pixiv_bookmark: 0,
    pixiv_account: 0,
    deletion_marker: 0,
    subscription: 0,
    system_setting: 0,
    administrator: 0
  });
  private connection: AppEventConnection | null = null;

  connect(): void {
    if (this.connection) {
      return;
    }
    this.connection = connectAppEvents({
      onInvalidate: (invalidation) => this.invalidate(invalidation)
    });
  }

  disconnect(): void {
    this.connection?.close();
    this.connection = null;
  }

  private invalidate(invalidation: AppInvalidation): void {
    if (invalidation.kind === 'snapshot') {
      this.snapshotRevision += 1;
      return;
    }
    this.resourceRevisions[invalidation.resource] += 1;
  }
}

export const appEventsStore = new AppEventsStore();
