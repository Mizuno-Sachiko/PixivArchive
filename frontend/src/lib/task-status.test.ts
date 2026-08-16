import { describe, expect, it } from 'vitest';

import { taskStateTone } from './task-status';

describe('task status tone', () => {
  it.each([
    { state: 'queued', tone: 'neutral' },
    { state: 'running', tone: 'primary' },
    { state: 'retry_wait', tone: 'warning' },
    { state: 'waiting_account', tone: 'error' },
    { state: 'waiting_storage', tone: 'warning' },
    { state: 'completed', tone: 'success' },
    { state: 'failed', tone: 'error' },
    { state: 'cancelled', tone: 'neutral' }
  ])('maps $state to $tone', ({ state, tone }) => {
    expect(taskStateTone(state)).toBe(tone);
  });

  it('keeps unknown task states neutral', () => {
    expect(taskStateTone('future_state')).toBe('neutral');
  });
});
