import { describe, expect, it } from 'vitest';

import { shouldMaskThumbnail } from './content-rating';

describe('content rating visibility', () => {
  it('masks R-18, R-18G and unknown ratings only when enabled', () => {
    expect(shouldMaskThumbnail('all_age', true)).toBe(false);
    expect(shouldMaskThumbnail('r18', true)).toBe(true);
    expect(shouldMaskThumbnail('r18g', true)).toBe(true);
    expect(shouldMaskThumbnail('unknown', true)).toBe(true);
    expect(shouldMaskThumbnail(null, true)).toBe(true);
    expect(shouldMaskThumbnail('r18', false)).toBe(false);
  });
});
