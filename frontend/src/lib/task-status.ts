export type TaskStateTone =
  'neutral' | 'success' | 'warning' | 'error' | 'primary';

const TASK_STATE_TONES: Readonly<Record<string, TaskStateTone>> = {
  queued: 'neutral',
  running: 'primary',
  retry_wait: 'warning',
  waiting_account: 'error',
  waiting_storage: 'warning',
  completed: 'success',
  failed: 'error',
  cancelled: 'neutral'
};

export function taskStateTone(state: string): TaskStateTone {
  return TASK_STATE_TONES[state] ?? 'neutral';
}
