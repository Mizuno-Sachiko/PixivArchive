export type RequestToken = symbol;

export class LatestRequest {
  private current: RequestToken = Symbol('initial request');

  begin(): RequestToken {
    this.current = Symbol('request');
    return this.current;
  }

  isCurrent(token: RequestToken): boolean {
    return token === this.current;
  }

  invalidate(): void {
    this.current = Symbol('invalidated request');
  }
}
