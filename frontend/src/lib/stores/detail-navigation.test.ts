import { describe, expect, it } from 'vitest';

import {
  clearDetailSource,
  currentDetailSource,
  detailReturnRoute,
  registerDetailSource
} from './detail-navigation';

describe('detail navigation source', () => {
  it('accepts only a source registered by the current browser runtime', () => {
    const source = registerDetailSource({
      kind: 'gallery',
      route: '/gallery/favorites'
    });

    expect(currentDetailSource({ detailSource: source })).toEqual(source);
    expect(detailReturnRoute({ detailSource: source })).toBe(
      '/gallery/favorites'
    );

    clearDetailSource(source.key);
    expect(currentDetailSource({ detailSource: source })).toBeNull();
    expect(detailReturnRoute({ detailSource: source })).toBe('/gallery');
  });

  it('rejects altered and unregistered history state', () => {
    const source = registerDetailSource({
      kind: 'trash',
      route: '/system/trash'
    });

    expect(
      currentDetailSource({
        detailSource: { ...source, route: '/gallery/favorites' }
      })
    ).toBeNull();
    expect(
      detailReturnRoute({
        detailSource: {
          key: 'unregistered',
          kind: 'gallery',
          route: '/gallery/favorites'
        }
      })
    ).toBe('/gallery');
    clearDetailSource(source.key);
  });
});
