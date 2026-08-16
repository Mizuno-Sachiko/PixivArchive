<script lang="ts">
  import { resolve } from '$app/paths';
  import type { Pathname } from '$app/types';
  import type { MouseEventHandler } from 'svelte/elements';

  import Icon from './Icon.svelte';

  interface Props {
    href: string;
    label: string;
    text?: string;
    external?: boolean;
    icon?: 'external-link' | 'arrow-right';
    size?: 'standard' | 'compact';
    toolbar?: boolean;
    onclick?: MouseEventHandler<HTMLAnchorElement>;
  }

  let {
    href,
    label,
    text,
    external = false,
    icon = 'external-link',
    size = 'standard',
    toolbar = false,
    onclick
  }: Props = $props();
</script>

{#snippet content()}
  <Icon name={icon} size={toolbar ? 17 : 16} />
  {#if text}<span>{text}</span>{/if}
{/snippet}

{#if external}
  <a
    class:text-link={Boolean(text)}
    class:compact={size === 'compact'}
    class:toolbar
    class="icon-link"
    {href}
    target="_blank"
    rel="external noopener noreferrer"
    aria-label={label}
    title={label}
    {onclick}
  >
    {@render content()}
  </a>
{:else}
  <a
    class:text-link={Boolean(text)}
    class:compact={size === 'compact'}
    class:toolbar
    class="icon-link"
    href={resolve(href as Pathname)}
    aria-label={label}
    title={label}
    {onclick}
  >
    {@render content()}
  </a>
{/if}

<style>
  .icon-link {
    display: inline-flex;
    width: var(--control-height-md);
    height: var(--control-height-md);
    flex: 0 0 auto;
    align-items: center;
    justify-content: center;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    background: var(--color-surface-2);
    color: var(--color-text-2);
    line-height: 0;
  }

  .icon-link :global(svg) {
    display: block;
  }

  .icon-link.compact {
    width: 36px;
    height: 36px;
  }

  .icon-link.text-link {
    width: auto;
    gap: 0.35rem;
    padding: 0 0.65rem;
    color: var(--color-primary);
    font-size: 0.75rem;
    font-weight: 700;
  }

  .icon-link:hover,
  .icon-link:focus-visible {
    border-color: color-mix(in srgb, var(--color-primary) 45%, transparent);
    background: var(--color-primary-soft);
    color: var(--color-primary);
  }

  .icon-link.toolbar {
    width: auto;
    min-width: 34px;
    height: 34px;
    padding: 0 0.55rem;
    border: 0;
    border-radius: var(--radius-pill);
    background: transparent;
    color: var(--viewer-control-text);
  }

  .icon-link.toolbar:hover,
  .icon-link.toolbar:focus-visible {
    border-color: transparent;
    background: var(--viewer-control-hover);
    color: var(--viewer-control-text);
  }
</style>
