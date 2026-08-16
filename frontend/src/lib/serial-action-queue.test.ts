import { describe, expect, it } from 'vitest';

import { SerialActionQueue } from './serial-action-queue';

describe('SerialActionQueue', () => {
  it('runs actions in submission order', async () => {
    const queue = new SerialActionQueue();
    const events: string[] = [];
    let releaseFirst!: () => void;
    const firstFinished = new Promise<void>((resolve) => {
      releaseFirst = resolve;
    });

    const first = queue.enqueue(async () => {
      events.push('first started');
      await firstFinished;
      events.push('first finished');
    });
    const second = queue.enqueue(async () => {
      events.push('second started');
    });

    await Promise.resolve();
    expect(events).toEqual(['first started']);
    releaseFirst();
    await Promise.all([first, second]);
    expect(events).toEqual([
      'first started',
      'first finished',
      'second started'
    ]);
  });

  it('continues with the next action after a failure', async () => {
    const queue = new SerialActionQueue();
    const failure = queue.enqueue(async () => {
      throw new Error('failed');
    });
    const result = queue.enqueue(async () => 'continued');

    await expect(failure).rejects.toThrow('failed');
    await expect(result).resolves.toBe('continued');
    await expect(queue.waitForIdle()).resolves.toBeUndefined();
  });
});
