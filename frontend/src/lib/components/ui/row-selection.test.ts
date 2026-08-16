import { describe, expect, it } from 'vitest';

import { isRowSelectionControl } from './row-selection';

describe('isRowSelectionControl', () => {
  it('recognizes controls and links while leaving row copy selectable', () => {
    expect(isRowSelectionControl({ closest: () => ({}) })).toBe(true);
    expect(isRowSelectionControl({ closest: () => null })).toBe(false);
    expect(isRowSelectionControl(null)).toBe(false);
  });
});
