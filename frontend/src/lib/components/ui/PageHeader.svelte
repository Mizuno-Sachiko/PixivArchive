<script lang="ts">
  import type { Snippet } from 'svelte';

  type HeaderVariant = 'workspace' | 'page' | 'gallery';

  interface Props {
    title: string;
    variant?: HeaderVariant;
    description?: string;
    actions?: Snippet;
    descriptionTools?: Snippet;
    showDescriptionTools?: boolean;
    class?: string;
  }

  let {
    title,
    variant = 'workspace',
    description,
    actions,
    descriptionTools,
    showDescriptionTools = true,
    class: className = ''
  }: Props = $props();
</script>

<header class={`${variant}-heading ${className}`.trim()}>
  {#if variant === 'gallery' && (actions || description)}
    <div class="gallery-title-row">
      <h1>{title}</h1>
      {#if actions}{@render actions()}{/if}
    </div>
    {#if description}
      <div class="gallery-description-row">
        {#if descriptionTools && showDescriptionTools}
          <div class="gallery-description-actions">
            {@render descriptionTools()}
          </div>
        {/if}
        <p>{description}</p>
      </div>
    {/if}
  {:else}
    <h1>{title}</h1>
    {#if actions}{@render actions()}{/if}
  {/if}
</header>

<style>
  .gallery-title-row {
    display: flex;
    min-width: 0;
    align-items: center;
    gap: 0.75rem;
  }

  .gallery-description-row {
    display: flex;
    align-items: center;
    gap: 0.55rem;
  }

  .gallery-description-actions {
    display: inline-flex;
    flex: 0 0 auto;
    align-items: center;
    gap: 0.5rem;
  }

  .gallery-description-row p {
    margin: 0;
  }

  @media (max-width: 720px) {
    .gallery-description-row {
      width: 100%;
      flex-wrap: wrap;
    }

    .gallery-description-actions {
      order: 2;
      margin-left: auto;
    }

    .gallery-description-row p {
      flex: 1 0 100%;
      order: 1;
    }
  }
</style>
