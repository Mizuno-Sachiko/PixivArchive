<script lang="ts">
  interface Props {
    count?: number;
  }

  let { count = 8 }: Props = $props();
</script>

{#each Array(count).keys() as index (`context-skeleton-${index}`)}
  <div class="context-card context-card-skeleton" aria-hidden="true">
    <span class="context-skeleton-line context-skeleton-title"></span>
    <span class="context-skeleton-line context-skeleton-meta"></span>
  </div>
{/each}
