export interface WaterfallItem {
  id: string;
  width: number;
  height: number;
  extraHeight?: number;
}

export interface WaterfallOptions {
  columnCount: number;
  columnWidth: number;
  gap: number;
  itemExtraHeight?: number;
}

export interface WaterfallPlacement {
  id: string;
  index: number;
  column: number;
  left: number;
  top: number;
  width: number;
  height: number;
  outerHeight: number;
}

export interface WaterfallLayout {
  placements: WaterfallPlacement[];
  totalHeight: number;
}

export function galleryCardChromeHeight(hasTags: boolean): number {
  return hasTags ? 66 : 48;
}

export function responsiveColumnCount(
  containerWidth: number,
  targetWidth = 220,
  gap = 16,
  minimum = 2,
  maximum = 8
): number {
  const available = Math.max(0, containerWidth);
  return Math.max(
    minimum,
    Math.min(maximum, Math.floor((available + gap) / (targetWidth + gap)))
  );
}

export function balancedPlacements(
  items: WaterfallItem[],
  options: WaterfallOptions
): WaterfallLayout {
  const itemExtraHeight = options.itemExtraHeight ?? 0;
  if (
    options.columnCount < 1 ||
    options.columnWidth <= 0 ||
    options.gap < 0 ||
    itemExtraHeight < 0
  ) {
    throw new RangeError('Waterfall dimensions must be positive');
  }

  const columnHeights = Array.from({ length: options.columnCount }, () => 0);
  const placements = items.map((item, index) => {
    if (item.width <= 0 || item.height <= 0) {
      throw new RangeError(`Waterfall item ${item.id} has invalid dimensions`);
    }
    const column = shortestColumn(columnHeights);
    const height = Math.round((item.height / item.width) * options.columnWidth);
    const outerHeight = height + (item.extraHeight ?? itemExtraHeight);
    const top = columnHeights[column];
    columnHeights[column] = top + outerHeight + options.gap;
    return {
      id: item.id,
      index,
      column,
      left: column * (options.columnWidth + options.gap),
      top,
      width: options.columnWidth,
      height,
      outerHeight
    };
  });

  return {
    placements,
    totalHeight:
      Math.max(0, ...columnHeights) - (items.length > 0 ? options.gap : 0)
  };
}

export function visiblePlacements(
  placements: WaterfallPlacement[],
  viewport: {
    scrollTop: number;
    viewportHeight: number;
    overscan: number;
  }
): WaterfallPlacement[] {
  const minimum = Math.max(0, viewport.scrollTop - viewport.overscan);
  const maximum =
    viewport.scrollTop + viewport.viewportHeight + viewport.overscan;
  return placements.filter(
    (placement) =>
      placement.top + placement.outerHeight >= minimum &&
      placement.top <= maximum
  );
}

function shortestColumn(heights: number[]): number {
  let selected = 0;
  for (let index = 1; index < heights.length; index += 1) {
    if (heights[index] < heights[selected]) {
      selected = index;
    }
  }
  return selected;
}
