<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    children: Snippet;
  }

  let { children }: Props = $props();
</script>

<div class="gallery-toolbar-surface">
  {@render children()}
</div>

<style>
  .gallery-toolbar-surface {
    display: flex;
    min-height: 64px;
    min-width: 0;
    align-items: center;
    flex-wrap: nowrap;
    gap: 0.65rem;
    padding: 0.6rem;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    background: var(--color-glass);
    box-shadow: 0 10px 32px rgba(24, 43, 63, 0.08);
    backdrop-filter: blur(18px) saturate(135%);
  }

  .gallery-toolbar-surface :global(form) {
    display: flex;
    min-width: 0;
    flex: 1 1 auto;
    align-items: center;
    flex-wrap: nowrap;
    gap: 0.65rem;
  }

  .gallery-toolbar-surface :global(input[type='search']) {
    min-width: 0;
    height: 42px;
    flex: 1 1 240px;
    padding: 0 0.85rem;
    border: 0;
    outline: 0;
    background: transparent;
  }

  .gallery-toolbar-surface :global(form > button) {
    flex: 0 0 auto;
    white-space: nowrap;
  }

  .gallery-toolbar-surface :global(form .pa-select-trigger) {
    min-width: 122px;
    white-space: nowrap;
  }

  @media (max-width: 900px) {
    .gallery-toolbar-surface {
      align-items: stretch;
    }

    .gallery-toolbar-surface :global(form) {
      display: grid;
      grid-template-columns: repeat(4, minmax(0, 1fr));
      align-items: center;
    }

    .gallery-toolbar-surface :global(input[type='search']) {
      grid-column: 1 / -1;
      width: 100%;
      flex: none;
    }

    .gallery-toolbar-surface :global(form .pa-select-trigger),
    .gallery-toolbar-surface :global(form > button) {
      width: 100%;
      min-width: 0;
    }
  }

  @media (max-width: 560px) {
    .gallery-toolbar-surface :global(form) {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }

    .gallery-toolbar-surface
      :global(form.gallery-search-controls > button[type='submit']) {
      grid-column: 1 / -1;
    }
  }
</style>
