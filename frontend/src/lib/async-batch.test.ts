import { describe, expect, it } from 'vitest';

import { settleWithConcurrency } from './async-batch';

describe('bounded batch execution', () => {
  it('preserves input order and never exceeds the requested concurrency', async () => {
    let active = 0;
    let maximumActive = 0;
    const releases = Array.from({ length: 4 }, () => deferred<void>());
    const operation = async (value: number, index: number) => {
      active += 1;
      maximumActive = Math.max(maximumActive, active);
      await releases[index].promise;
      active -= 1;
      if (value === 2) throw new Error('failed');
      return value * 10;
    };

    const running = settleWithConcurrency([1, 2, 3, 4], operation, 2);
    await Promise.resolve();
    expect(maximumActive).toBe(2);
    releases[1].resolve();
    await Promise.resolve();
    releases[0].resolve();
    await Promise.resolve();
    releases[2].resolve();
    releases[3].resolve();

    const results = await running;
    expect(maximumActive).toBe(2);
    expect(results.map((result) => result.status)).toEqual([
      'fulfilled',
      'rejected',
      'fulfilled',
      'fulfilled'
    ]);
    expect(results[0]).toEqual({ status: 'fulfilled', value: 10 });
  });
});

function deferred<Value>() {
  let resolve!: (value: Value) => void;
  const promise = new Promise<Value>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}
