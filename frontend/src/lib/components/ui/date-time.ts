import {
  CalendarDateTime,
  getLocalTimeZone,
  type DateValue
} from '@internationalized/date';

export interface DateTimeSelectOption {
  value: string;
  label: string;
}

interface DateTimeParts {
  year?: number;
  month?: number;
  day?: number;
  hour?: number;
  minute?: number;
}

export function dateTimeFromIso(value: string): CalendarDateTime | undefined {
  if (!value) return undefined;
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return undefined;
  return calendarDateTime(date);
}

export function dateTimePlaceholder(now = new Date()): CalendarDateTime {
  return calendarDateTime(now);
}

export function dateTimeToIso(value: DateValue): string {
  return value.toDate(getLocalTimeZone()).toISOString();
}

export function updateDateTime(
  value: DateValue,
  parts: DateTimeParts
): CalendarDateTime {
  const time = dateTimeClock(value);
  return new CalendarDateTime(
    parts.year ?? value.year,
    parts.month ?? value.month,
    parts.day ?? value.day,
    parts.hour ?? time.hour,
    parts.minute ?? time.minute
  );
}

export function dateTimeClock(value: DateValue): {
  hour: number;
  minute: number;
} {
  return {
    hour: 'hour' in value ? value.hour : 0,
    minute: 'minute' in value ? value.minute : 0
  };
}

export function dateTimeYearOptions(
  currentYear = new Date().getFullYear()
): DateTimeSelectOption[] {
  const earliestYear = 1900;
  const latestYear = currentYear + 20;
  return Array.from(
    { length: latestYear - earliestYear + 1 },
    (_, index) => latestYear - index
  ).map((year) => ({ value: String(year), label: `${year}年` }));
}

export const dateTimeMonthOptions = numberedOptions(
  12,
  (value) => `${value}月`
);
export const dateTimeHourOptions = numberedOptions(
  24,
  (value) => `${value.toString().padStart(2, '0')}时`,
  0
);
export const dateTimeMinuteOptions = numberedOptions(
  60,
  (value) => `${value.toString().padStart(2, '0')}分`,
  0
);

function calendarDateTime(value: Date): CalendarDateTime {
  return new CalendarDateTime(
    value.getFullYear(),
    value.getMonth() + 1,
    value.getDate(),
    value.getHours(),
    value.getMinutes()
  );
}

function numberedOptions(
  count: number,
  label: (value: number) => string,
  start = 1
): DateTimeSelectOption[] {
  return Array.from({ length: count }, (_, index) => index + start).map(
    (value) => ({ value: String(value), label: label(value) })
  );
}
