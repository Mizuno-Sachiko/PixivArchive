import type { PixivAgeRating } from '$lib/api/gallery';

export function shouldMaskThumbnail(
  ageRating: PixivAgeRating | null | undefined,
  maskingEnabled: boolean
): boolean {
  return maskingEnabled && ageRating !== 'all_age';
}
