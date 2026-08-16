import { SvelteMap } from 'svelte/reactivity';

import {
  followingApi,
  type FollowingApi,
  type FollowingAuthor,
  type FollowingState
} from '$lib/api/following';
import { SerialActionQueue } from '$lib/serial-action-queue';

interface OperationContext {
  accountId: string;
  generation: number;
}

interface AuthorIntent {
  enabled: boolean;
  revision: number;
}

interface SubscriptionIntent {
  enabled: boolean;
  intervalMinutes: number;
  revision: number;
}

export class FollowingStateSession {
  state = $state<FollowingState | null>(null);
  loading = $state(true);
  loadError = $state('');
  loadRetryable = $state(false);
  accountMismatch = $state(false);
  intervalMinutes = $state('15');

  private accountId: string | null = null;
  private generation = 0;
  private intentRevision = 0;
  private loadRevision = 0;
  private serverState: FollowingState | null = null;
  private queue = new SerialActionQueue();
  private pendingSubscription: SubscriptionIntent | null = null;
  private readonly pendingAuthors = new SvelteMap<number, AuthorIntent>();

  constructor(private readonly api: FollowingApi = followingApi) {}

  setAccount(accountId: string | null): boolean {
    if (this.accountId === accountId) return false;
    this.generation += 1;
    this.accountId = accountId;
    this.serverState = null;
    this.state = null;
    this.queue = new SerialActionQueue();
    this.pendingSubscription = null;
    this.pendingAuthors.clear();
    this.intervalMinutes = '15';
    this.loadError = accountId ? '' : '请先配置Pixiv账户';
    this.loadRetryable = false;
    this.accountMismatch = false;
    this.loading = Boolean(accountId);
    return true;
  }

  dispose(): void {
    this.generation += 1;
    this.queue = new SerialActionQueue();
  }

  async load(): Promise<boolean> {
    const context = this.context();
    if (!context) {
      this.loading = false;
      this.loadError = '请先配置Pixiv账户';
      this.loadRetryable = false;
      return true;
    }
    const loadRevision = ++this.loadRevision;
    this.loading = true;
    this.loadError = '';
    this.loadRetryable = false;
    this.accountMismatch = false;
    return this.queue.enqueue(async () => {
      if (!this.isCurrent(context)) return false;
      try {
        const loaded = await this.api.get();
        return this.applyFullState(loaded, context);
      } catch {
        if (this.isCurrent(context) && loadRevision === this.loadRevision) {
          this.loadError = '关注列表暂时无法读取';
          this.loadRetryable = true;
        }
        return false;
      } finally {
        if (this.isCurrent(context) && loadRevision === this.loadRevision) {
          this.loading = false;
        }
      }
    });
  }

  async updateSubscription(
    enabled: boolean,
    intervalMinutes: number
  ): Promise<boolean> {
    const context = this.context();
    if (!context || !this.serverState) return false;
    const revision = ++this.intentRevision;
    this.pendingSubscription = { enabled, intervalMinutes, revision };
    this.intervalMinutes = String(intervalMinutes);
    this.publish();

    return this.queue.enqueue(async () => {
      if (!this.isCurrent(context) || !this.serverState) return false;
      try {
        const subscription = await this.api.updateSubscription(
          enabled,
          intervalMinutes,
          this.serverState.subscription.revision,
          context.accountId
        );
        if (!this.isCurrent(context)) return false;
        if (subscription.account_id !== context.accountId) {
          return this.rejectAccountMismatch();
        }
        this.serverState = { ...this.serverState, subscription };
        const current = this.pendingSubscription?.revision === revision;
        if (current) {
          this.pendingSubscription = null;
          this.intervalMinutes = String(subscription.schedule.interval_minutes);
        }
        this.publish();
        return current;
      } catch (cause) {
        if (!this.isCurrent(context)) return false;
        if (this.pendingSubscription?.revision !== revision) return false;
        this.pendingSubscription = null;
        this.intervalMinutes = String(
          this.serverState.subscription.schedule.interval_minutes
        );
        this.publish();
        throw cause;
      }
    });
  }

  async updateAuthor(
    pixivArtistId: number,
    enabled: boolean
  ): Promise<boolean> {
    const context = this.context();
    if (!context || !this.serverState) return false;
    const revision = ++this.intentRevision;
    this.pendingAuthors.set(pixivArtistId, { enabled, revision });
    this.publish();

    return this.queue.enqueue(async () => {
      if (!this.isCurrent(context) || !this.serverState) return false;
      try {
        const updated = await this.api.updateAuthor(
          pixivArtistId,
          enabled,
          context.accountId
        );
        if (!this.isCurrent(context)) return false;
        this.replaceServerAuthor(updated);
        const current =
          this.pendingAuthors.get(pixivArtistId)?.revision === revision;
        if (current) this.pendingAuthors.delete(pixivArtistId);
        this.publish();
        return current;
      } catch (cause) {
        if (!this.isCurrent(context)) return false;
        if (this.pendingAuthors.get(pixivArtistId)?.revision !== revision) {
          return false;
        }
        this.pendingAuthors.delete(pixivArtistId);
        this.publish();
        throw cause;
      }
    });
  }

  async updateAuthors(
    pixivArtistIds: number[],
    enabled: boolean
  ): Promise<boolean> {
    const context = this.context();
    if (!context || !this.serverState || pixivArtistIds.length === 0) {
      return false;
    }
    const revision = ++this.intentRevision;
    for (const pixivArtistId of pixivArtistIds) {
      this.pendingAuthors.set(pixivArtistId, { enabled, revision });
    }
    this.publish();

    return this.queue.enqueue(async () => {
      if (!this.isCurrent(context) || !this.serverState) return false;
      try {
        const updated = await this.api.updateAuthors(
          pixivArtistIds,
          enabled,
          context.accountId
        );
        if (!this.isCurrent(context)) return false;
        if (updated.subscription.account_id !== context.accountId) {
          return this.rejectAccountMismatch();
        }
        this.serverState = updated;
        const current = pixivArtistIds.every(
          (pixivArtistId) =>
            this.pendingAuthors.get(pixivArtistId)?.revision === revision
        );
        this.clearAuthorIntents(pixivArtistIds, revision);
        this.publish();
        return current;
      } catch (cause) {
        if (!this.isCurrent(context)) return false;
        const current = pixivArtistIds.some(
          (pixivArtistId) =>
            this.pendingAuthors.get(pixivArtistId)?.revision === revision
        );
        this.clearAuthorIntents(pixivArtistIds, revision);
        this.publish();
        if (!current) return false;
        throw cause;
      }
    });
  }

  async refresh(): Promise<boolean> {
    const context = this.context();
    if (!context) return false;
    return this.queue.enqueue(async () => {
      if (!this.isCurrent(context)) return false;
      try {
        const refreshed = await this.api.refresh(context.accountId);
        return this.applyFullState(refreshed, context);
      } catch (cause) {
        if (!this.isCurrent(context)) return false;
        throw cause;
      }
    });
  }

  async run(backfill = false): Promise<boolean> {
    const context = this.context();
    if (!context) return false;
    return this.queue.enqueue(async () => {
      if (!this.isCurrent(context)) return false;
      try {
        await this.api.run(context.accountId, backfill);
        return this.isCurrent(context);
      } catch (cause) {
        if (!this.isCurrent(context)) return false;
        throw cause;
      }
    });
  }

  private applyFullState(
    state: FollowingState,
    context: OperationContext
  ): boolean {
    if (!this.isCurrent(context)) return false;
    if (state.subscription.account_id !== context.accountId) {
      return this.rejectAccountMismatch();
    }
    this.serverState = state;
    this.accountMismatch = false;
    if (!this.pendingSubscription) {
      this.intervalMinutes = String(
        state.subscription.schedule.interval_minutes
      );
    }
    this.publish();
    return true;
  }

  private rejectAccountMismatch(): false {
    this.serverState = null;
    this.state = null;
    this.pendingSubscription = null;
    this.pendingAuthors.clear();
    this.intervalMinutes = '15';
    this.loadError = '当前Pixiv账户已经变化，正在重新读取';
    this.loadRetryable = true;
    this.accountMismatch = true;
    return false;
  }

  private replaceServerAuthor(updated: FollowingAuthor): void {
    if (!this.serverState) return;
    this.serverState = {
      ...this.serverState,
      authors: this.serverState.authors.map((author) =>
        author.pixiv_artist_id === updated.pixiv_artist_id ? updated : author
      )
    };
  }

  private clearAuthorIntents(pixivArtistIds: number[], revision: number): void {
    for (const pixivArtistId of pixivArtistIds) {
      if (this.pendingAuthors.get(pixivArtistId)?.revision === revision) {
        this.pendingAuthors.delete(pixivArtistId);
      }
    }
  }

  private publish(): void {
    if (!this.serverState) {
      this.state = null;
      return;
    }
    const subscription = this.pendingSubscription
      ? {
          ...this.serverState.subscription,
          enabled: this.pendingSubscription.enabled,
          schedule: {
            ...this.serverState.subscription.schedule,
            interval_minutes: this.pendingSubscription.intervalMinutes
          }
        }
      : this.serverState.subscription;
    this.state = {
      ...this.serverState,
      subscription,
      authors: this.serverState.authors.map((author) => {
        const intent = this.pendingAuthors.get(author.pixiv_artist_id);
        return intent ? { ...author, enabled: intent.enabled } : author;
      })
    };
  }

  private context(): OperationContext | null {
    return this.accountId
      ? { accountId: this.accountId, generation: this.generation }
      : null;
  }

  private isCurrent(context: OperationContext): boolean {
    return (
      context.generation === this.generation &&
      context.accountId === this.accountId
    );
  }
}
