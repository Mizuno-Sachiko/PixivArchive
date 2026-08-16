import { describe, expect, it } from 'vitest';

import { LatestRequest } from './latest-request';

describe('LatestRequest', () => {
  it('keeps only the most recently started request current', () => {
    const requests = new LatestRequest();
    const first = requests.begin();
    const second = requests.begin();

    expect(requests.isCurrent(first)).toBe(false);
    expect(requests.isCurrent(second)).toBe(true);
  });

  it('invalidates a pending request without starting another operation', () => {
    const requests = new LatestRequest();
    const pending = requests.begin();

    requests.invalidate();

    expect(requests.isCurrent(pending)).toBe(false);
  });
});
