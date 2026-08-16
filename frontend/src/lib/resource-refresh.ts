type RefreshCallback = () => Promise<boolean>;

export interface ResourceRefreshOptions<Version> {
  sameVersion: (left: Version, right: Version) => boolean;
  mergeVersions?: (latest: Version, current: Version) => Version;
  delay?: number;
}

export class ResourceRefreshCoordinator<Version> {
  private active: boolean;
  private disposed = false;
  private running = false;
  private paused = false;
  private timer: ReturnType<typeof setTimeout> | undefined;
  private generation = 0;
  private retryRequested = false;
  private currentVersion: Version | null;
  private latestVersion: Version | null;
  private failedVersion: Version | null = null;

  constructor(
    initialVersion: Version | null,
    private readonly refresh: RefreshCallback,
    private readonly options: ResourceRefreshOptions<Version>
  ) {
    this.active = initialVersion !== null;
    this.currentVersion = initialVersion;
    this.latestVersion = initialVersion;
  }

  start(version: Version): void {
    if (this.disposed) return;
    this.active = true;
    this.observe(version);
  }

  observe(version: Version, paused = false): void {
    if (this.disposed) return;
    this.latestVersion = version;
    this.paused = paused;
    if (this.matches(version, this.currentVersion)) {
      this.failedVersion = null;
      if (!this.running) this.cancelTimer();
      return;
    }
    if (this.matches(version, this.failedVersion)) {
      if (!this.running) this.cancelTimer();
      return;
    }
    this.schedule();
  }

  markCurrent(current: Version): void {
    if (this.disposed) return;
    this.generation += 1;
    this.currentVersion = current;
    this.latestVersion =
      this.latestVersion !== null
        ? (this.options.mergeVersions?.(this.latestVersion, current) ?? current)
        : current;
    this.failedVersion = null;
    this.cancelTimer();
    this.schedule();
  }

  retry(): void {
    if (this.disposed) return;
    if (this.running) {
      this.retryRequested = true;
      return;
    }
    if (this.failedVersion === null) return;
    this.failedVersion = null;
    this.cancelTimer();
    if (!this.paused && !this.running) {
      void this.runRefresh();
    }
  }

  dispose(): void {
    this.disposed = true;
    this.generation += 1;
    this.retryRequested = false;
    this.cancelTimer();
  }

  private schedule(): void {
    if (
      this.disposed ||
      !this.active ||
      this.paused ||
      this.running ||
      this.timer ||
      this.latestVersion === null ||
      this.matches(this.latestVersion, this.currentVersion) ||
      this.matches(this.latestVersion, this.failedVersion)
    ) {
      return;
    }

    const delay = this.options.delay ?? 0;
    if (delay > 0) {
      this.timer = setTimeout(() => {
        this.timer = undefined;
        void this.runRefresh();
      }, delay);
      return;
    }
    void this.runRefresh();
  }

  private async runRefresh(): Promise<void> {
    if (
      this.disposed ||
      !this.active ||
      this.paused ||
      this.running ||
      this.latestVersion === null ||
      this.matches(this.latestVersion, this.currentVersion) ||
      this.matches(this.latestVersion, this.failedVersion)
    ) {
      return;
    }

    const targetVersion = this.latestVersion;
    const generation = this.generation;
    this.running = true;
    let succeeded: boolean;
    try {
      succeeded = await this.refresh();
    } catch {
      succeeded = false;
    }
    this.running = false;
    if (this.disposed) return;
    const retryRequested = this.retryRequested;
    this.retryRequested = false;
    if (generation === this.generation) {
      if (succeeded) {
        this.currentVersion = targetVersion;
        this.failedVersion = null;
      } else {
        this.failedVersion = retryRequested ? null : targetVersion;
      }
    }
    this.schedule();
  }

  private matches(left: Version, right: Version | null): boolean {
    return right !== null && this.options.sameVersion(left, right);
  }

  private cancelTimer(): void {
    if (this.timer === undefined) return;
    clearTimeout(this.timer);
    this.timer = undefined;
  }
}
