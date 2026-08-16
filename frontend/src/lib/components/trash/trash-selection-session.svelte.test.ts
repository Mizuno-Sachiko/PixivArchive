import { describe, expect, it } from 'vitest';

import { TrashSelectionSession } from './trash-selection-session.svelte';

const first = '0198f64c-42a2-7374-bace-9f1c3b317fc1';
const second = '0198f64c-42a2-7374-bace-9f1c3b317fc2';
const outside = '0198f64c-42a2-7374-bace-9f1c3b317fc3';

describe('TrashSelectionSession', () => {
  it('fixes the filter on entry and projects complete-range operations', async () => {
    const requests: unknown[] = [];
    const session = new TrashSelectionSession({
      project: async (request) => {
        requests.push(structuredClone(request));
        return {
          selected_count: request.expression.base_selected ? 501 : 1,
          blocked_count: 0,
          selected_visible_work_ids: request.expression.base_selected
            ? [first, second]
            : [outside]
        };
      }
    });
    session.enter({ query: '待清理', purge_states: ['failed'] });

    await session.selectAll([first, second]);
    await session.invert([first, second, outside]);

    expect(session.count).toBe(1);
    expect(session.idsFor([first, second, outside])).toEqual(
      new Set([outside])
    );
    expect(requests).toEqual([
      {
        expression: {
          filter: { query: '待清理', purge_states: ['failed'] },
          base_selected: true,
          exception_work_ids: []
        },
        visible_work_ids: [first, second]
      },
      {
        expression: {
          filter: { query: '待清理', purge_states: ['failed'] },
          base_selected: false,
          exception_work_ids: []
        },
        visible_work_ids: [first, second, outside]
      }
    ]);
  });

  it('clears the complete selection without requesting the server', async () => {
    let requests = 0;
    const session = new TrashSelectionSession({
      project: async (request) => {
        requests += 1;
        return {
          selected_count: (request.expression.exception_work_ids ?? []).length,
          blocked_count: 0,
          selected_visible_work_ids: request.expression.exception_work_ids ?? []
        };
      }
    });
    session.enter({ query: null, purge_states: [] });
    await session.setWork(first, true, [first, second]);
    await session.setWork(second, true, [first, second]);

    session.clear();

    expect(session.count).toBe(0);
    expect(session.idsFor([first, second])).toEqual(new Set());
    expect(requests).toBe(2);
  });

  it('keeps the prior expression when a projection fails', async () => {
    let failNext = false;
    const session = new TrashSelectionSession({
      project: async (request) => {
        if (failNext) throw new Error('unavailable');
        return {
          selected_count: 1,
          blocked_count: 0,
          selected_visible_work_ids: request.expression.exception_work_ids ?? []
        };
      }
    });
    session.enter({ query: null, purge_states: [] });
    await session.setWork(outside, true, [outside]);
    failNext = true;

    await session.selectAll([outside]);

    expect(session.snapshotExpression()).toEqual({
      filter: { query: null, purge_states: [] },
      base_selected: false,
      exception_work_ids: [outside]
    });
    expect(session.count).toBe(1);
    expect(session.error).toBe('无法更新当前选择');
  });

  it('accepts a projection larger than 500 works without loading every id', async () => {
    const session = new TrashSelectionSession({
      project: async () => ({
        selected_count: 1200,
        blocked_count: 0,
        selected_visible_work_ids: [first]
      })
    });
    session.enter({ query: null, purge_states: [] });

    await session.selectAll([first]);

    expect(session.count).toBe(1200);
    expect(session.idsFor([first])).toEqual(new Set([first]));
    expect(session.snapshotExpression().base_selected).toBe(true);
  });

  it('reprojects newly visible works from the fixed selection expression', async () => {
    const requests: unknown[] = [];
    const session = new TrashSelectionSession({
      project: async (request) => {
        requests.push(structuredClone(request));
        return {
          selected_count: 1200,
          blocked_count: 0,
          selected_visible_work_ids: [...request.visible_work_ids]
        };
      }
    });
    session.enter({ query: '待清理', purge_states: ['pending'] });
    await session.selectAll([first]);

    await session.refreshVisible([first, second]);

    expect(session.count).toBe(1200);
    expect(session.idsFor([first, second])).toEqual(new Set([first, second]));
    expect(requests).toEqual([
      {
        expression: {
          filter: { query: '待清理', purge_states: ['pending'] },
          base_selected: true,
          exception_work_ids: []
        },
        visible_work_ids: [first]
      },
      {
        expression: {
          filter: { query: '待清理', purge_states: ['pending'] },
          base_selected: true,
          exception_work_ids: []
        },
        visible_work_ids: [first, second]
      }
    ]);
  });

  it('projects the latest visible page after a pending selection commits', async () => {
    let resolveSelection!: (value: {
      selected_count: number;
      blocked_count: number;
      selected_visible_work_ids: string[];
    }) => void;
    let resolveVisible!: (value: {
      selected_count: number;
      blocked_count: number;
      selected_visible_work_ids: string[];
    }) => void;
    const selectionProjection = new Promise<{
      selected_count: number;
      blocked_count: number;
      selected_visible_work_ids: string[];
    }>((resolve) => {
      resolveSelection = resolve;
    });
    const visibleProjection = new Promise<{
      selected_count: number;
      blocked_count: number;
      selected_visible_work_ids: string[];
    }>((resolve) => {
      resolveVisible = resolve;
    });
    const requests: unknown[] = [];
    const session = new TrashSelectionSession({
      project: (request) => {
        requests.push(structuredClone(request));
        return requests.length === 1 ? selectionProjection : visibleProjection;
      }
    });
    session.enter({ query: null, purge_states: [] });

    const selecting = session.selectAll([first]);
    const refreshing = session.refreshVisible([first, second]);
    expect(requests).toHaveLength(1);

    resolveSelection({
      selected_count: 2,
      blocked_count: 0,
      selected_visible_work_ids: [first]
    });
    await selecting;
    await Promise.resolve();
    expect(requests).toHaveLength(2);

    resolveVisible({
      selected_count: 2,
      blocked_count: 0,
      selected_visible_work_ids: [first, second]
    });
    await refreshing;

    expect(session.idsFor([first, second])).toEqual(new Set([first, second]));
  });

  it('invalidates a pending range operation after multi-select exits', async () => {
    let resolveProjection!: (value: {
      selected_count: number;
      blocked_count: number;
      selected_visible_work_ids: string[];
    }) => void;
    const projection = new Promise<{
      selected_count: number;
      blocked_count: number;
      selected_visible_work_ids: string[];
    }>((resolve) => {
      resolveProjection = resolve;
    });
    const session = new TrashSelectionSession({
      project: () => projection
    });
    session.enter({ query: null, purge_states: [] });

    const selecting = session.selectAll([first]);
    session.exit();
    resolveProjection({
      selected_count: 1,
      blocked_count: 0,
      selected_visible_work_ids: [first]
    });
    await selecting;

    expect(session.mode).toBe(false);
    expect(session.count).toBe(0);
    expect(session.idsFor([first])).toEqual(new Set());
  });

  it('keeps the latest trash selection when projections finish out of order', async () => {
    const firstProjection = deferred<{
      selected_count: number;
      blocked_count: number;
      selected_visible_work_ids: string[];
    }>();
    const secondProjection = deferred<{
      selected_count: number;
      blocked_count: number;
      selected_visible_work_ids: string[];
    }>();
    let request = 0;
    const session = new TrashSelectionSession({
      project: () =>
        request++ === 0 ? firstProjection.promise : secondProjection.promise
    });
    session.enter({ query: null, purge_states: [] });

    const selectingFirst = session.setWork(first, true, [first, second]);
    expect(session.idsFor([first, second])).toEqual(new Set([first]));
    const selectingSecond = session.setWork(second, true, [first, second]);
    expect(request).toBe(2);
    expect(session.idsFor([first, second])).toEqual(new Set([first, second]));

    secondProjection.resolve({
      selected_count: 2,
      blocked_count: 1,
      selected_visible_work_ids: [first, second]
    });
    await selectingSecond;
    firstProjection.resolve({
      selected_count: 1,
      blocked_count: 0,
      selected_visible_work_ids: [first]
    });
    await selectingFirst;

    expect(session.count).toBe(2);
    expect(session.blockedCount).toBe(1);
    expect(session.idsFor([first, second])).toEqual(new Set([first, second]));
    expect(session.snapshotExpression().exception_work_ids).toEqual([
      first,
      second
    ]);
  });

  it('restores the local trash intent that preceded a failed latest projection', async () => {
    const firstProjection = deferred<{
      selected_count: number;
      blocked_count: number;
      selected_visible_work_ids: string[];
    }>();
    let request = 0;
    const session = new TrashSelectionSession({
      project: async () => {
        if (request++ === 0) return firstProjection.promise;
        throw new Error('unavailable');
      }
    });
    session.enter({ query: null, purge_states: [] });

    const selectingFirst = session.setWork(first, true, [first, second]);
    await session.setWork(second, true, [first, second]);

    expect(session.count).toBe(1);
    expect(session.idsFor([first, second])).toEqual(new Set([first]));
    expect(session.snapshotExpression().exception_work_ids).toEqual([first]);
    expect(session.error).toBe('无法更新当前选择');

    firstProjection.resolve({
      selected_count: 1,
      blocked_count: 0,
      selected_visible_work_ids: [first]
    });
    await selectingFirst;
  });
});

function deferred<T>(): {
  promise: Promise<T>;
  resolve: (value: T) => void;
} {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((next) => {
    resolve = next;
  });
  return { promise, resolve };
}
