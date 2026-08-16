<script lang="ts">
  import { onMount, type Snippet } from 'svelte';

  import '$lib/styles/app.css';
  import { themeStore } from '$lib/stores/theme.svelte';

  interface Props {
    children: Snippet;
  }

  let { children }: Props = $props();

  onMount(() => themeStore.initialize());
</script>

{@render children()}
