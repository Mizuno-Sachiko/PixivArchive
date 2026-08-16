import { render } from 'svelte/server';
import { describe, expect, it } from 'vitest';

import type { EffectiveSettings } from '$lib/api/system';

import CollectionSettingsCard from './CollectionSettingsCard.svelte';

describe('collection settings media capabilities', () => {
  it('hides AVIF controls when AVIF derivatives are unavailable', () => {
    const view = render(CollectionSettingsCard, {
      props: props(false, 'webp')
    });

    expect(view.body).not.toContain('默认浏览图格式');
    expect(view.body).not.toContain('AVIF质量');
    expect(view.body).toContain('WebP质量');
  });

  it('shows the format selector and selected quality when AVIF is available', () => {
    const view = render(CollectionSettingsCard, {
      props: props(true, 'avif')
    });

    expect(view.body).toContain('默认浏览图格式');
    expect(view.body).toContain('AVIF质量');
    expect(view.body).not.toContain('WebP质量');
  });
});

function props(
  avifAvailable: boolean,
  format: EffectiveSettings['derivative']['format']
) {
  return {
    retry: { network_backoff_seconds: [60, 300, 1_200, 3_600] },
    derivative: {
      format,
      max_width: 768,
      webp_quality: 80,
      avif_quality: 50
    },
    revisions: { retry: undefined, derivative: undefined },
    avifAvailable,
    onsaved: () => undefined
  };
}
