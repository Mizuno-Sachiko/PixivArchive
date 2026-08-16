import { systemApi, type PixivAccount } from '$lib/api/system';
import { LatestRequest } from '$lib/latest-request';
import { isPixivAccountAvailable } from '$lib/pixiv-account-status';

class PixivAccountStore {
  current = $state<PixivAccount | null>(null);
  loading = $state(false);
  error = $state('');
  private readonly requests = new LatestRequest();

  get currentForAction(): PixivAccount | null {
    return !this.current || !isPixivAccountAvailable(this.current.state)
      ? null
      : this.current;
  }

  async load(): Promise<PixivAccount | null> {
    const request = this.requests.begin();
    this.loading = true;
    this.error = '';
    try {
      const account = await systemApi.account();
      if (!this.requests.isCurrent(request)) return this.current;
      this.current = account;
      return account;
    } catch {
      if (this.requests.isCurrent(request)) {
        this.error = 'Pixiv账户资料暂时无法读取';
      }
      return this.current;
    } finally {
      if (this.requests.isCurrent(request)) this.loading = false;
    }
  }

  replace(account: PixivAccount): void {
    this.requests.invalidate();
    this.current = account;
    this.error = '';
    this.loading = false;
  }

  reset(): void {
    this.requests.invalidate();
    this.current = null;
    this.error = '';
    this.loading = false;
  }
}

export const pixivAccountStore = new PixivAccountStore();
