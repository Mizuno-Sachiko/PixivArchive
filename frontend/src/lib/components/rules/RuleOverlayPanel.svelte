<script lang="ts">
  import type { Snippet } from 'svelte';

  import Icon from '$lib/components/ui/Icon.svelte';

  interface Props {
    title: string;
    closeLabel: string;
    onClose: () => void;
    children: Snippet;
  }

  let { title, closeLabel, onClose, children }: Props = $props();
</script>

<div class="sheet">
  <header>
    <h3>{title}</h3>
    <button type="button" aria-label={closeLabel} onclick={onClose}>
      <Icon name="close" size={16} />
    </button>
  </header>
  {@render children()}
</div>

<style>
  .sheet {
    position: absolute;
    z-index: 7;
    inset: 0;
    display: grid;
    align-content: start;
    gap: 0.8rem;
    padding: 1rem;
    background: var(--color-glass-strong);
    backdrop-filter: blur(18px);
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  h3 {
    margin: 0;
  }

  button {
    display: grid;
    width: 30px;
    height: 30px;
    place-items: center;
    border-radius: 7px;
    background: var(--color-surface-2);
  }
</style>
