import type { Pathname } from '$app/types';

export function galleryWorkPath(pixivWorkId: number): Pathname {
  return `/gallery/works/${pixivWorkId}` as Pathname;
}

export function galleryArtistPath(pixivArtistId: number): Pathname {
  return `/gallery/artists/${pixivArtistId}` as Pathname;
}

export function gallerySeriesPath(pixivSeriesId: number): Pathname {
  return `/gallery/series/${pixivSeriesId}` as Pathname;
}

export function galleryTagPath(tagName: string): Pathname {
  return `/gallery/tags/${encodeURIComponent(tagName.trim())}` as Pathname;
}

export function parseSourceId(value: string | undefined): number | null {
  if (!value || !/^\d+$/.test(value)) return null;
  const id = Number(value);
  return Number.isSafeInteger(id) && id > 0 ? id : null;
}
