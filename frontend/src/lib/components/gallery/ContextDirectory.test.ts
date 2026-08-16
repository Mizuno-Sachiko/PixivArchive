import { describe, expect, it } from 'vitest';

import { retryResetsDirectory } from './ContextDirectory.svelte';

describe('context directory retry', () => {
  it('restarts a failed search from its first page', () => {
    expect(retryResetsDirectory('reset')).toBe(true);
  });

  it('continues from the current cursor after pagination fails', () => {
    expect(retryResetsDirectory('next')).toBe(false);
  });
});
