<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    title: string;
    level?: 2 | 3;
    eyebrow?: string;
    subtitle?: string;
    count?: string | number;
    actions?: Snippet;
    titleWrapped?: boolean;
    class?: string;
  }

  let {
    title,
    level = 2,
    eyebrow,
    subtitle,
    count,
    actions,
    titleWrapped = true,
    class: className = ''
  }: Props = $props();
</script>

{#snippet heading()}
  {#if eyebrow}<p class="eyebrow">{eyebrow}</p>{/if}
  {#if level === 3}<h3>{title}</h3>{:else}<h2>{title}</h2>{/if}
  {#if subtitle}<p>{subtitle}</p>{/if}
{/snippet}

<header class={`panel-heading ${className}`.trim()}>
  {#if titleWrapped}
    <div>{@render heading()}</div>
  {:else}
    {@render heading()}
  {/if}
  {#if count !== undefined}<span class="panel-count">{count}</span>{/if}
  {#if actions}{@render actions()}{/if}
</header>
