import { CalendarDateTime } from '@internationalized/date';
import { describe, expect, it } from 'vitest';

import { dateTimeFromIso, dateTimeToIso } from './date-time';

describe('date-time conversion', () => {
  it('presents an ISO instant in local calendar time', () => {
    const source = new Date(2026, 7, 25, 10, 30);
    expect(dateTimeFromIso(source.toISOString())?.toString()).toBe(
      '2026-08-25T10:30:00'
    );
  });

  it('serializes a selected local calendar time as an ISO instant', () => {
    const selected = new CalendarDateTime(2026, 8, 25, 10, 30);
    expect(dateTimeToIso(selected)).toBe(
      new Date(2026, 7, 25, 10, 30).toISOString()
    );
  });

  it('does not manufacture a value from invalid input', () => {
    expect(dateTimeFromIso('not-a-date')).toBeUndefined();
  });
});
