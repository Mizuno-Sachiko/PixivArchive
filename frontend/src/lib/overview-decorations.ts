import type { OverviewDecoration } from '$lib/api/gallery';

const slotCount = 3;

export interface OverviewDecorationSlot {
  decoration: OverviewDecoration | null;
}

export function dailyDecorationKey(date = new Date()): string {
  const year = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, '0');
  const day = String(date.getDate()).padStart(2, '0');
  return `${year}-${month}-${day}`;
}

export function decorationSlots(
  items: readonly (OverviewDecoration | null)[]
): OverviewDecorationSlot[] {
  return Array.from({ length: slotCount }, (_, index) => ({
    decoration: items[index] ?? null
  }));
}
