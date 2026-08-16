import { describe, expect, it } from 'vitest';

import {
  accumulateWheelDelta,
  centeredScrollOffset,
  clampZoom,
  dominantColorLuminance,
  mediaAspect,
  normalizedScrollCenter
} from './viewer-math';

describe('viewer math', () => {
  it('normalizes valid and missing media dimensions', () => {
    expect(mediaAspect(2400, 1200)).toBe(2);
    expect(mediaAspect(1200, 2400)).toBe(0.5);
    expect(mediaAspect(0, 2400)).toBe(1);
    expect(mediaAspect()).toBe(1);
  });

  it('keeps the same content center after zoom changes scroll dimensions', () => {
    const center = normalizedScrollCenter({
      scrollLeft: 500,
      scrollTop: 250,
      scrollWidth: 2000,
      scrollHeight: 1200,
      clientWidth: 1000,
      clientHeight: 600
    });
    const offset = centeredScrollOffset(center, {
      scrollWidth: 4000,
      scrollHeight: 2400,
      clientWidth: 1000,
      clientHeight: 600
    });

    expect(center).toEqual({ x: 0.5, y: 0.4583333333333333 });
    expect(offset).toEqual({ x: 1500, y: 800 });
  });

  it('clamps zoom and emits one page direction at the wheel threshold', () => {
    expect(clampZoom(0.1, 0.25, 4)).toBe(0.25);
    expect(clampZoom(6, 0.25, 4)).toBe(4);
    expect(accumulateWheelDelta(35, 30, 80)).toEqual({
      accumulated: 65,
      direction: 0
    });
    expect(accumulateWheelDelta(65, 20, 80)).toEqual({
      accumulated: 0,
      direction: 1
    });
    expect(accumulateWheelDelta(-55, -25, 80)).toEqual({
      accumulated: 0,
      direction: -1
    });
  });

  it('computes relative luminance for control contrast', () => {
    expect(dominantColorLuminance('#ffffff')).toBeCloseTo(1);
    expect(dominantColorLuminance('#000000')).toBe(0);
    expect(dominantColorLuminance('#808080')).toBeCloseTo(0.216, 3);
    expect(dominantColorLuminance('var(--color-viewer-bg)')).toBe(0);
  });
});
