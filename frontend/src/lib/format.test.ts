import { describe, expect, it } from 'vitest';

import {
  formatCount,
  formatDateTime,
  formatDateTimeTitle,
  formatExactCount,
  formatExactDateTime
} from './format';

describe('count formatting', () => {
  it('keeps small totals exact and compacts large totals', () => {
    expect(formatCount(9_999)).toBe('9,999');
    expect(formatCount(12_500)).toBe('1.3万');
    expect(formatExactCount(1_234_567)).toBe('1,234,567');
  });
});

function localIso(
  year: number,
  month: number,
  day: number,
  hour: number,
  minute: number,
  second = 0
): string {
  return new Date(year, month - 1, day, hour, minute, second).toISOString();
}

describe('date and time formatting', () => {
  const now = new Date(2026, 6, 31, 18, 0, 0);

  it('uses natural list dates in the browser timezone', () => {
    expect(formatDateTime(localIso(2026, 7, 31, 14, 26), now)).toBe(
      '今天 14:26'
    );
    expect(formatDateTime(localIso(2026, 7, 30, 21, 8), now)).toBe(
      '昨天 21:08'
    );
    expect(formatDateTime(localIso(2026, 7, 29, 11, 59), now)).toBe(
      '7月29日 11:59'
    );
    expect(formatDateTime(localIso(2025, 12, 31, 23, 59), now)).toBe(
      '2025年12月31日 23:59'
    );
  });

  it('uses a precise detail value and a timezone-aware title', () => {
    const value = localIso(2026, 7, 31, 14, 26, 32);
    expect(formatExactDateTime(value)).toBe('2026年7月31日 14:26:32');
    expect(formatDateTimeTitle(value)).toContain('2026');
  });
});
