import { describe, expect, it } from 'vitest';

import {
  balancedPlacements,
  galleryCardChromeHeight,
  responsiveColumnCount,
  visiblePlacements
} from './waterfall';

describe('balanced waterfall placement', () => {
  const items = [
    { id: 'a', width: 400, height: 800 },
    { id: 'b', width: 400, height: 400 },
    { id: 'c', width: 400, height: 600 },
    { id: 'd', width: 400, height: 300 }
  ];

  it('always places the next card in the shortest column', () => {
    const layout = balancedPlacements(items, {
      columnCount: 2,
      columnWidth: 200,
      gap: 16
    });

    expect(
      layout.placements.map(({ id, column, top }) => ({
        id,
        column,
        top
      }))
    ).toEqual([
      { id: 'a', column: 0, top: 0 },
      { id: 'b', column: 1, top: 0 },
      { id: 'c', column: 1, top: 216 },
      { id: 'd', column: 0, top: 416 }
    ]);
    expect(layout.totalHeight).toBe(566);
  });

  it('includes card chrome when advancing each column height', () => {
    const layout = balancedPlacements(
      [
        { id: 'a', width: 400, height: 400 },
        { id: 'b', width: 400, height: 400 }
      ],
      {
        columnCount: 1,
        columnWidth: 100,
        gap: 16,
        itemExtraHeight: 86
      }
    );

    expect(layout.placements).toEqual([
      {
        id: 'a',
        index: 0,
        column: 0,
        left: 0,
        top: 0,
        width: 100,
        height: 100,
        outerHeight: 186
      },
      {
        id: 'b',
        index: 1,
        column: 0,
        left: 0,
        top: 202,
        width: 100,
        height: 100,
        outerHeight: 186
      }
    ]);
    expect(layout.totalHeight).toBe(388);
  });

  it('uses deterministic card copy heights for every loaded card', () => {
    expect(galleryCardChromeHeight(false)).toBe(48);
    expect(galleryCardChromeHeight(true)).toBe(66);
  });

  it('recomputes the column count from the current container width', () => {
    expect(responsiveColumnCount(420)).toBe(2);
    expect(responsiveColumnCount(720)).toBe(3);
    expect(responsiveColumnCount(1_200)).toBe(5);
    expect(responsiveColumnCount(2_000)).toBe(8);
  });

  it('keeps an overscanned visible window without changing placement', () => {
    const layout = balancedPlacements(items, {
      columnCount: 2,
      columnWidth: 200,
      gap: 16
    });

    expect(
      visiblePlacements(layout.placements, {
        scrollTop: 200,
        viewportHeight: 150,
        overscan: 20
      }).map((placement) => placement.id)
    ).toEqual(['a', 'b', 'c']);
  });
});
