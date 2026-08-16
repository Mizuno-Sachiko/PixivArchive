<script lang="ts">
  import { onMount, tick, untrack } from 'svelte';

  interface Props {
    enabled?: boolean;
    hasMore: boolean;
    loading: boolean;
    onLoadMore: () => void;
    preloadViewports?: number;
  }

  let {
    enabled = true,
    hasMore,
    loading,
    onLoadMore,
    preloadViewports = 3
  }: Props = $props();
  let boundary = $state<HTMLDivElement>();
  let withinPreloadRange = $state(false);
  let updateQueued = false;

  function queueRangeUpdate(): void {
    if (updateQueued) return;
    updateQueued = true;
    requestAnimationFrame(() => {
      updateQueued = false;
      updateRange();
    });
  }

  function updateRange(): void {
    if (!boundary || !enabled) {
      withinPreloadRange = false;
      return;
    }
    const bounds = boundary.getBoundingClientRect();
    const preloadDistance = window.innerHeight * preloadViewports;
    withinPreloadRange =
      bounds.top <= window.innerHeight + preloadDistance &&
      bounds.bottom >= -preloadDistance;
  }

  $effect(() => {
    void enabled;
    void hasMore;
    void loading;
    untrack(() => {
      void tick().then(queueRangeUpdate);
    });
  });

  $effect(() => {
    if (enabled && withinPreloadRange && hasMore && !loading) {
      untrack(onLoadMore);
    }
  });

  onMount(() => {
    const resizeObserver = new ResizeObserver(queueRangeUpdate);
    resizeObserver.observe(document.documentElement);
    window.addEventListener('scroll', queueRangeUpdate, { passive: true });
    window.addEventListener('resize', queueRangeUpdate, { passive: true });
    queueRangeUpdate();
    return () => {
      resizeObserver.disconnect();
      window.removeEventListener('scroll', queueRangeUpdate);
      window.removeEventListener('resize', queueRangeUpdate);
    };
  });
</script>

<div
  class="viewport-page-boundary"
  bind:this={boundary}
  aria-hidden="true"
></div>

<style>
  .viewport-page-boundary {
    width: 100%;
    height: 1px;
    pointer-events: none;
  }
</style>
