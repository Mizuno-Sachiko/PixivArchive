import { describe, expect, it } from 'vitest';

import { FollowingSelectionSession } from './following-selection.svelte';

describe('FollowingSelectionSession', () => {
  it('selects loaded authors without affecting authors outside the current list', () => {
    const selection = new FollowingSelectionSession();
    selection.enter();

    selection.toggle(101);
    selection.selectAll([101, 102, 103]);
    selection.toggle(102);

    expect(selection.ids).toEqual(new Set([101, 103]));
    expect(selection.count).toBe(2);
  });

  it('inverts and clears the current loaded author selection', () => {
    const selection = new FollowingSelectionSession();
    selection.enter();
    selection.selectAll([101, 102]);

    selection.invert([102, 103]);
    expect(selection.ids).toEqual(new Set([101, 103]));

    selection.clear();
    expect(selection.ids).toEqual(new Set());
  });

  it('removes authors that no longer exist after a list refresh', () => {
    const selection = new FollowingSelectionSession();
    selection.enter();
    selection.selectAll([101, 102, 103]);

    selection.retain([102, 103, 104]);

    expect(selection.ids).toEqual(new Set([102, 103]));
  });
});
