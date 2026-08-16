<script lang="ts">
  import type { PixivAgeRating } from '$lib/api/gallery';

  import WorkThumbnail from '../ui/WorkThumbnail.svelte';

  interface Props {
    src?: string | null;
    ageRating?: PixivAgeRating | null;
  }

  let { src = null, ageRating = null }: Props = $props();
</script>

<div class="context-thumbnail">
  <WorkThumbnail {src} {ageRating} />
</div>

<style>
  .context-thumbnail {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
  }

  .context-thumbnail :global(.thumbnail-unavailable) {
    position: relative;
    z-index: 2;
  }
</style>
