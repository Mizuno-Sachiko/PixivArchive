export interface Point {
  x: number;
  y: number;
}

export interface ScrollGeometry {
  scrollLeft: number;
  scrollTop: number;
  scrollWidth: number;
  scrollHeight: number;
  clientWidth: number;
  clientHeight: number;
}

export function clampZoom(
  value: number,
  minimum: number,
  maximum: number
): number {
  return Math.max(minimum, Math.min(maximum, value));
}

export function mediaAspect(
  width?: number | null,
  height?: number | null
): number {
  return width && height && width > 0 && height > 0 ? width / height : 1;
}

export function normalizedScrollCenter(geometry: ScrollGeometry): Point {
  return {
    x:
      (geometry.scrollLeft + geometry.clientWidth / 2) /
      Math.max(geometry.scrollWidth, geometry.clientWidth),
    y:
      (geometry.scrollTop + geometry.clientHeight / 2) /
      Math.max(geometry.scrollHeight, geometry.clientHeight)
  };
}

export function centeredScrollOffset(
  center: Point,
  geometry: Pick<
    ScrollGeometry,
    'scrollWidth' | 'scrollHeight' | 'clientWidth' | 'clientHeight'
  >
): Point {
  return {
    x: center.x * geometry.scrollWidth - geometry.clientWidth / 2,
    y: center.y * geometry.scrollHeight - geometry.clientHeight / 2
  };
}

export function accumulateWheelDelta(
  accumulated: number,
  delta: number,
  threshold: number
): { accumulated: number; direction: -1 | 0 | 1 } {
  const next = accumulated + delta;
  if (Math.abs(next) < threshold) {
    return { accumulated: next, direction: 0 };
  }
  return { accumulated: 0, direction: next > 0 ? 1 : -1 };
}

export function dominantColorLuminance(color: string): number {
  const match = /^#([\da-f]{2})([\da-f]{2})([\da-f]{2})$/i.exec(color);
  if (!match) return 0;
  const channels = match.slice(1).map((value) => {
    const channel = Number.parseInt(value, 16) / 255;
    return channel <= 0.04045
      ? channel / 12.92
      : ((channel + 0.055) / 1.055) ** 2.4;
  });
  return channels[0] * 0.2126 + channels[1] * 0.7152 + channels[2] * 0.0722;
}
