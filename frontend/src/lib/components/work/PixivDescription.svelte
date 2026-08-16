<script lang="ts">
  import { renderPixivDescription } from '$lib/pixiv-description';

  interface Props {
    value: string;
  }

  let { value }: Props = $props();
  let container = $state<HTMLDivElement>();

  $effect(() => {
    if (container) renderPixivDescription(container, value);
  });
</script>

<div class="description" bind:this={container}></div>

<style>
  .description {
    min-width: 0;
    color: var(--color-text-2);
    font-size: 0.8rem;
    line-height: 1.8;
    overflow-wrap: anywhere;
    white-space: pre-wrap;
  }

  :global(.description p) {
    margin: 0 0 0.85rem;
    white-space: pre-wrap;
  }

  :global(.description p:last-child) {
    margin-bottom: 0;
  }

  :global(.description a) {
    color: var(--color-primary);
    text-decoration: underline;
    text-decoration-color: color-mix(
      in srgb,
      var(--color-primary) 40%,
      transparent
    );
    text-underline-offset: 0.16em;
    overflow-wrap: anywhere;
  }
</style>
