import { describe, expect, it } from 'vitest';

import {
  clearSelection,
  invertSelection,
  isSelected,
  selectAll,
  setSelected
} from './selection-expression';

describe('selection expression state', () => {
  it('uses exceptions as the inverse of the base selection state', () => {
    const selected = setSelected(
      { base_selected: false, exception_ids: [] },
      'one',
      true
    );

    expect(selected).toEqual({
      base_selected: false,
      exception_ids: ['one']
    });
    expect(isSelected(selected, 'one')).toBe(true);
    expect(isSelected(selected, 'two')).toBe(false);

    const removed = setSelected(selected, 'one', false);
    expect(removed.exception_ids).toEqual([]);
  });

  it('selects, clears and inverts a complete fixed query without listing it', () => {
    const selected = selectAll();
    expect(selected).toEqual({ base_selected: true, exception_ids: [] });

    const excluded = setSelected(selected, 'two', false);
    expect(invertSelection(excluded)).toEqual({
      base_selected: false,
      exception_ids: ['two']
    });
    expect(clearSelection()).toEqual({
      base_selected: false,
      exception_ids: []
    });
  });

  it('does not mutate the previous expression', () => {
    const current = {
      base_selected: true,
      exception_ids: ['one']
    };

    setSelected(current, 'two', false);

    expect(current).toEqual({
      base_selected: true,
      exception_ids: ['one']
    });
  });
});
