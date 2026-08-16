<script lang="ts">
  import { resolve } from '$app/paths';
  import type { Pathname } from '$app/types';

  import type { OverviewDecoration } from '$lib/api/gallery';
  import WorkThumbnail from '$lib/components/ui/WorkThumbnail.svelte';

  interface Props {
    href: Pathname;
    label: string;
    decoration: OverviewDecoration | null;
    maskNonAllAge: boolean;
  }

  let { href, label, decoration, maskNonAllAge }: Props = $props();
</script>

<a href={resolve(href)} data-decoration-work-id={decoration?.pixiv_work_id}>
  <span class="quick-art" aria-hidden="true">
    <WorkThumbnail
      src={decoration?.cover_url}
      alt=""
      ageRating={decoration?.age_rating}
      {maskNonAllAge}
      unavailableMessage="暂无可用装饰图"
      zoom
    />
  </span>
  <span class="quick-shade" aria-hidden="true"></span>
  <strong>{label}</strong>
</a>

<style>
  a {
    position: relative;
    display: grid;
    min-height: var(--overview-quick-link-min-height, 112px);
    align-content: end;
    padding: 0.9rem;
    overflow: hidden;
    border: 1px solid var(--color-border);
    border-radius: 11px;
    background: var(--color-surface-1);
    isolation: isolate;
  }

  a::before {
    position: absolute;
    z-index: 3;
    top: 0;
    bottom: 0;
    left: 0;
    width: 3px;
    background: var(--color-primary);
    content: '';
    opacity: 0;
    transform: scaleY(0.35);
    transition:
      opacity var(--motion-fast),
      transform var(--motion-base) var(--ease-standard);
  }

  a:hover::before,
  a:focus-visible::before {
    opacity: 1;
    transform: scaleY(1);
  }

  a:hover :global(.work-thumbnail.zoom img) {
    transform: scale(1.025);
  }

  .quick-art,
  .quick-shade {
    position: absolute;
    inset: 0;
  }

  .quick-art {
    z-index: -2;
  }

  .quick-shade {
    z-index: -1;
    background: linear-gradient(
      180deg,
      color-mix(in srgb, var(--color-surface-1) 8%, transparent) 42%,
      color-mix(in srgb, var(--color-surface-1) 70%, transparent) 100%
    );
  }

  strong {
    display: block;
    margin-bottom: 8px;
    color: var(--color-text-1);
    font-size: 0.84rem;
    text-shadow: 0 1px 10px var(--color-surface-1);
  }
</style>
