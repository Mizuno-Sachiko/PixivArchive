export class SerialActionQueue {
  private tail: Promise<void> = Promise.resolve();

  enqueue<T>(action: () => Promise<T>): Promise<T> {
    const result = this.tail.then(action);
    this.tail = result.then(
      () => undefined,
      () => undefined
    );
    return result;
  }

  waitForIdle(): Promise<void> {
    return this.tail;
  }
}
