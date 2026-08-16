<script lang="ts">
  import { resolve } from '$app/paths';
  import type { Pathname } from '$app/types';

  import type { PixivAgeRating } from '$lib/api/gallery';
  import { formatCount, formatExactCount } from '$lib/format';

  import ContextThumbnail from './ContextThumbnail.svelte';

  interface Props {
    id: string;
    href: Pathname;
    anchor: string;
    title: string;
    eyebrow?: string;
    secondary?: string;
    workCount: number;
    coverUrl?: string | null;
    coverAgeRating?: PixivAgeRating | null;
    selectionMode?: boolean;
    selected?: boolean;
    onSelect?: (id: string, selected: boolean) => void;
    onOpen?: () => void;
  }

  let {
    id,
    href,
    anchor,
    title,
    eyebrow,
    secondary,
    workCount,
    coverUrl = null,
    coverAgeRating = null,
    selectionMode = false,
    selected = false,
    onSelect,
    onOpen
  }: Props = $props();

  function select(): void {
    onSelect?.(id, !selected);
  }
</script>

{#snippet content()}
  <ContextThumbnail src={coverUrl} ageRating={coverAgeRating} />
  <span class="context-copy">
    {#if eyebrow}<small>{eyebrow}</small>{/if}
    <strong>{title}</strong>
    <span title={`${formatExactCount(workCount)}件作品`}>
      {#if secondary}{secondary} ·
      {/if}{formatCount(workCount)}件作品
    </span>
  </span>
{/snippet}

<article
  class="context-card"
  class:has-cover={Boolean(coverUrl)}
  class:selection-mode={selectionMode}
  class:selected
  data-gallery-anchor={anchor}
>
  {@render content()}
  {#if selectionMode}
    <button
      class="context-interaction"
      type="button"
      aria-label={`选择${title}`}
      aria-pressed={selected}
      onclick={select}
    ></button>
    <span class="context-selection-control" aria-hidden="true">
      <input type="checkbox" checked={selected} tabindex="-1" />
    </span>
  {:else}
    <a
      class="context-interaction"
      href={resolve(href)}
      aria-label={`打开${title}`}
      onclick={onOpen}
    ></a>
  {/if}
</article>

<style>
  .context-card.selected {
    border-color: var(--color-primary);
    box-shadow: inset 0 0 0 2px var(--color-primary);
  }

  .context-card.selection-mode {
    cursor: pointer;
  }

  .context-interaction {
    position: absolute;
    z-index: 4;
    inset: 0;
    display: block;
    width: 100%;
    padding: 0;
    background: transparent;
  }

  .context-selection-control {
    position: absolute;
    z-index: 5;
    top: 0.65rem;
    right: 0.65rem;
    display: grid;
    width: 30px;
    height: 30px;
    place-content: center;
    border-radius: 50%;
    background: rgba(0, 0, 0, 0.68);
    backdrop-filter: blur(10px);
  }

  .context-selection-control input {
    width: 17px;
    height: 17px;
    accent-color: var(--color-primary);
    pointer-events: none;
  }
</style>
