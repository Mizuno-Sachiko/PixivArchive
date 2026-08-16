const timeOptions = {
  hour: '2-digit',
  minute: '2-digit',
  hour12: false
} as const;

const exactNumberFormatter = new Intl.NumberFormat('zh-CN');
const compactNumberFormatter = new Intl.NumberFormat('zh-CN', {
  notation: 'compact',
  maximumFractionDigits: 1
});

function normalizeCount(value: number): number {
  return Number.isFinite(value) ? Math.max(0, Math.trunc(value)) : 0;
}

export function formatCount(value: number): string {
  const normalized = normalizeCount(value);
  return normalized < 10_000
    ? exactNumberFormatter.format(normalized)
    : compactNumberFormatter.format(normalized);
}

export function formatExactCount(value: number): string {
  return exactNumberFormatter.format(normalizeCount(value));
}

function parseDate(value: string | null): Date | null {
  if (!value) return null;
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? null : date;
}

function sameLocalDate(left: Date, right: Date): boolean {
  return (
    left.getFullYear() === right.getFullYear() &&
    left.getMonth() === right.getMonth() &&
    left.getDate() === right.getDate()
  );
}

function formatLocalDate(date: Date, includeYear: boolean): string {
  const year = includeYear ? `${date.getFullYear()}年` : '';
  return `${year}${date.getMonth() + 1}月${date.getDate()}日`;
}

export function formatDateTime(value: string | null, now = new Date()): string {
  const date = parseDate(value);
  if (!date) return value || '—';

  const time = new Intl.DateTimeFormat('zh-CN', timeOptions).format(date);
  if (sameLocalDate(date, now)) return `今天 ${time}`;

  const yesterday = new Date(now);
  yesterday.setDate(now.getDate() - 1);
  if (sameLocalDate(date, yesterday)) return `昨天 ${time}`;

  return `${formatLocalDate(date, date.getFullYear() !== now.getFullYear())} ${time}`;
}

export function formatExactDateTime(value: string | null): string {
  const date = parseDate(value);
  if (!date) return value || '—';
  const time = new Intl.DateTimeFormat('zh-CN', {
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
    hour12: false
  }).format(date);
  return `${formatLocalDate(date, true)} ${time}`;
}

export function formatDateTimeTitle(value: string | null): string | undefined {
  const date = parseDate(value);
  if (!date) return undefined;
  return new Intl.DateTimeFormat('zh-CN', {
    dateStyle: 'full',
    timeStyle: 'long'
  }).format(date);
}

export function formatBytes(value: number): string {
  if (!Number.isFinite(value) || value < 0) return '—';
  const units = ['B', 'KiB', 'MiB', 'GiB', 'TiB'];
  let size = value;
  let unit = 0;
  while (size >= 1024 && unit < units.length - 1) {
    size /= 1024;
    unit += 1;
  }
  return `${size >= 100 || unit === 0 ? size.toFixed(0) : size.toFixed(1)} ${units[unit]}`;
}

export function formatElapsed(start: string, end: string | null): string {
  if (!end) return '仍在执行';
  const elapsed = new Date(end).getTime() - new Date(start).getTime();
  if (!Number.isFinite(elapsed) || elapsed < 0) return '—';
  if (elapsed < 1000) return `${elapsed}毫秒`;
  if (elapsed < 60_000) return `${Math.round(elapsed / 1000)}秒`;
  return `${Math.round(elapsed / 60_000)}分钟`;
}
