export const MIN_SUBSCRIPTION_INTERVAL_MINUTES = 15;
export const MAX_SUBSCRIPTION_INTERVAL_MINUTES = 43_200;
export const MAX_SUBSCRIPTION_LOOKBACK_PAGES = 7;

export function subscriptionScheduleError(
  intervalMinutes: number,
  lookbackPages: number
): string | null {
  if (
    !Number.isInteger(intervalMinutes) ||
    intervalMinutes < MIN_SUBSCRIPTION_INTERVAL_MINUTES ||
    intervalMinutes > MAX_SUBSCRIPTION_INTERVAL_MINUTES
  ) {
    return '执行间隔必须在15到43200分钟之间';
  }
  if (
    !Number.isInteger(lookbackPages) ||
    lookbackPages < 0 ||
    lookbackPages > MAX_SUBSCRIPTION_LOOKBACK_PAGES
  ) {
    return '补采最近多少期必须在0到7之间';
  }
  return null;
}
