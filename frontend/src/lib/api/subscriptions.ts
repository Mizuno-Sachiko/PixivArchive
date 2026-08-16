import { apiRequest, type ApiRequest } from './client';
import type { components } from './schema';

export type SubscriptionKind = components['schemas']['SubscriptionKindDto'];
export type SubscriptionRecentState =
  components['schemas']['SubscriptionRecentState'];
export type Subscription = components['schemas']['SubscriptionDto'];
export type SubscriptionUpdate =
  components['schemas']['UpdateSubscriptionBody'];
export type SubscriptionRun = components['schemas']['SubscriptionRunDto'];
export type SubscriptionCursor = components['schemas']['SubscriptionCursorDto'];
export type SubscriptionRunAccepted =
  components['schemas']['SubscriptionRunAccepted'];

export interface SubscriptionApi {
  list(): Promise<Subscription[]>;
  get(id: string): Promise<Subscription>;
  create(
    input: components['schemas']['CreateSubscriptionBody']
  ): Promise<Subscription>;
  update(id: string, input: SubscriptionUpdate): Promise<Subscription>;
  setEnabled(
    id: string,
    expectedRevision: number,
    enabled: boolean
  ): Promise<Subscription>;
  remove(id: string, expectedRevision: number): Promise<void>;
  run(id: string, backfill?: boolean): Promise<SubscriptionRunAccepted>;
  stop(id: string): Promise<Subscription>;
  runs(id: string): Promise<SubscriptionRun[]>;
  cursors(id: string): Promise<SubscriptionCursor[]>;
}

export function createSubscriptionApi(
  request: ApiRequest = apiRequest
): SubscriptionApi {
  return {
    async list() {
      const response =
        await request<components['schemas']['SubscriptionList']>(
          '/api/subscriptions'
        );
      return response.items;
    },
    get(id) {
      return request(`/api/subscriptions/${id}`);
    },
    create(input) {
      return request('/api/subscriptions', { method: 'POST', json: input });
    },
    update(id, input) {
      return request(`/api/subscriptions/${id}`, {
        method: 'PUT',
        json: input
      });
    },
    setEnabled(id, expectedRevision, enabled) {
      return request(`/api/subscriptions/${id}/enabled`, {
        method: 'PUT',
        json: { expected_revision: expectedRevision, enabled }
      });
    },
    remove(id, expectedRevision) {
      return request(
        `/api/subscriptions/${id}?expected_revision=${expectedRevision}`,
        { method: 'DELETE' }
      );
    },
    run(id, backfill = false) {
      return request(`/api/subscriptions/${id}/run`, {
        method: 'POST',
        json: { backfill }
      });
    },
    stop(id) {
      return request(`/api/subscriptions/${id}/stop`, { method: 'POST' });
    },
    async runs(id) {
      const response = await request<
        components['schemas']['SubscriptionRunList']
      >(`/api/subscriptions/${id}/runs?limit=50`);
      return response.items;
    },
    async cursors(id) {
      const response = await request<
        components['schemas']['SubscriptionCursorList']
      >(`/api/subscriptions/${id}/cursors`);
      return response.items;
    }
  };
}

export const subscriptionApi = createSubscriptionApi();
