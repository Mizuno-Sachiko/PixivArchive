import { describe, expect, it } from 'vitest';

import {
  galleryArtistPath,
  gallerySeriesPath,
  galleryTagPath,
  galleryWorkPath,
  parseSourceId
} from './gallery-routes';

describe('gallery source routes', () => {
  it('uses Pixiv numeric identities for works, artists and series', () => {
    expect(galleryWorkPath(120001)).toBe('/gallery/works/120001');
    expect(galleryArtistPath(10001)).toBe('/gallery/artists/10001');
    expect(gallerySeriesPath(31001)).toBe('/gallery/series/31001');
  });

  it('keeps tag identities readable and safe as one URL segment', () => {
    expect(galleryTagPath(' ハメ撮り/視点 #1 ')).toBe(
      '/gallery/tags/%E3%83%8F%E3%83%A1%E6%92%AE%E3%82%8A%2F%E8%A6%96%E7%82%B9%20%231'
    );
  });

  it('accepts only positive safe numeric route identities', () => {
    expect(parseSourceId('120001')).toBe(120001);
    expect(parseSourceId('123abc')).toBeNull();
    expect(parseSourceId('12.5')).toBeNull();
    expect(parseSourceId('1e3')).toBeNull();
    expect(parseSourceId(' 123')).toBeNull();
    expect(parseSourceId('+123')).toBeNull();
    expect(parseSourceId('0')).toBeNull();
    expect(parseSourceId('-1')).toBeNull();
    expect(parseSourceId(String(Number.MAX_SAFE_INTEGER + 1))).toBeNull();
    expect(parseSourceId('not-an-id')).toBeNull();
  });
});
