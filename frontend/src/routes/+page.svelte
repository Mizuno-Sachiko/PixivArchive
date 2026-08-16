<script lang="ts">
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { onMount } from 'svelte';

  onMount(() => {
    void goto(resolve('/overview'), { replaceState: true });
  });
</script>

<svelte:head><title>PixivArchive</title></svelte:head>

<div class="route-loading" aria-label="正在进入PixivArchive"></div>

<style>
  .route-loading {
    width: 28px;
    height: 3px;
    margin: 48vh auto 0;
    overflow: hidden;
    border-radius: var(--radius-pill);
    background: var(--color-surface-3);
  }

  .route-loading::after {
    display: block;
    width: 45%;
    height: 100%;
    background: var(--color-primary);
    content: '';
    animation: loading 800ms var(--ease-standard) infinite alternate;
  }

  @keyframes loading {
    to {
      transform: translateX(122%);
    }
  }
</style>
