import { describe, expect, it } from 'vitest';

import type { OverviewDecoration } from '$lib/api/gallery';

import { dailyDecorationKey, decorationSlots } from './overview-decorations';

describe('overview decorations', () => {
  it('uses the browser-local calendar day', () => {
    const date = new Date(2026, 7, 12, 23, 59, 0);
    expect(dailyDecorationKey(date)).toBe('2026-08-12');
  });

  it('preserves hidden slot positions instead of silently selecting again', () => {
    const safe = decoration(1, 'all_age');
    const restricted = decoration(2, 'r18');
    const stored = [safe, restricted, safe];

    expect(decorationSlots(stored)).toEqual([
      { decoration: safe },
      { decoration: restricted },
      { decoration: safe }
    ]);
    expect(decorationSlots([safe, null, restricted])).toEqual([
      { decoration: safe },
      { decoration: null },
      { decoration: restricted }
    ]);
  });
});

function decoration(
  pixivWorkId: number,
  ageRating: OverviewDecoration['age_rating']
): OverviewDecoration {
  return {
    pixiv_work_id: pixivWorkId,
    title: `作品${pixivWorkId}`,
    age_rating: ageRating,
    cover_url: `/api/derivatives/${pixivWorkId}`
  };
}
