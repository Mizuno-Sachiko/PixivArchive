<script lang="ts">
  import type { Snippet } from 'svelte';

  import PanelHeader from '$lib/components/ui/PanelHeader.svelte';

  interface Props {
    title: string;
    description?: string;
    headerActions?: Snippet;
    actions?: Snippet;
    help?: Snippet;
    feedback?: Snippet;
    children: Snippet;
    class?: string;
  }

  let {
    title,
    description,
    headerActions,
    actions,
    help,
    feedback,
    children,
    class: className = ''
  }: Props = $props();
</script>

<section class={`panel settings-card ${className}`.trim()} data-settings-card>
  <PanelHeader {title} subtitle={description} actions={headerActions} />
  <div class="form-body">
    {@render children()}
    {#if actions}<div class="settings-card-actions">
        {@render actions()}
      </div>{/if}
    {#if help}<small class="settings-card-help">{@render help()}</small>{/if}
    {#if feedback}{@render feedback()}{/if}
  </div>
</section>

<style>
  .settings-card-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 0.65rem;
  }

  .settings-card-help {
    color: var(--color-text-3);
    font-size: 0.72rem;
    line-height: 1.55;
  }
</style>
