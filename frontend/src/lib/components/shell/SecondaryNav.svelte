<script lang="ts">
  import { resolve } from '$app/paths';
  import { page } from '$app/state';

  import Icon from '$lib/components/ui/Icon.svelte';

  interface Item {
    label: string;
    href: `/${string}`;
  }

  interface Props {
    items: Item[];
    trailingAction?: Item;
  }

  let { items, trailingAction }: Props = $props();
  let menuOpen = $state(false);
  let activeItem = $derived.by(() => {
    const pathname = page.url.pathname;
    return (
      items.find((item) => pathname === item.href) ??
      items
        .filter(
          (item) =>
            item.href !== '/gallery' && pathname.startsWith(`${item.href}/`)
        )
        .sort((left, right) => right.href.length - left.href.length)[0] ??
      items[0]
    );
  });

  function isActive(href: string): boolean {
    return activeItem?.href === href;
  }
</script>

{#snippet trailingActionLink()}
  {#if trailingAction}
    <a
      class="secondary-action"
      href={resolve(trailingAction.href)}
      aria-label={trailingAction.label}
    >
      <Icon name="arrow-left" size={17} />
      <span>{trailingAction.label}</span>
    </a>
  {/if}
{/snippet}

<nav class="secondary-nav" aria-label="二级导航">
  <div class="secondary-inner">
    {#each items as item (item.href)}
      <a class:active={isActive(item.href)} href={resolve(item.href)}
        >{item.label}</a
      >
    {/each}
    {@render trailingActionLink()}
  </div>

  <div class="secondary-selector">
    <button
      type="button"
      aria-expanded={menuOpen}
      aria-controls="secondary-menu"
      onclick={() => (menuOpen = !menuOpen)}
    >
      <span>{activeItem?.label ?? items[0]?.label}</span>
      <Icon name="chevron" size={18} />
    </button>
    {@render trailingActionLink()}
    {#if menuOpen}
      <div id="secondary-menu" class="secondary-menu">
        {#each items as item (item.href)}
          <a
            class:active={isActive(item.href)}
            href={resolve(item.href)}
            onclick={() => (menuOpen = false)}>{item.label}</a
          >
        {/each}
      </div>
    {/if}
  </div>
</nav>

<style>
  .secondary-nav {
    position: sticky;
    z-index: 30;
    top: var(--topbar-height);
    height: var(--secondary-nav-height);
    overflow: visible;
    border-bottom: 1px solid var(--color-border);
    background: color-mix(in srgb, var(--color-glass) 82%, transparent);
    box-shadow: 0 8px 24px rgba(24, 43, 63, 0.045);
    backdrop-filter: blur(16px) saturate(125%);
  }

  .secondary-inner {
    display: flex;
    width: min(var(--content-width), 100%);
    height: 100%;
    align-items: center;
    gap: 1.5rem;
    padding: 0 24px;
    margin: 0 auto;
    overflow: visible;
  }

  a {
    position: relative;
    display: grid;
    height: 100%;
    place-items: center;
    color: var(--color-text-2);
    font-size: 0.84rem;
    font-weight: 620;
    white-space: nowrap;
  }

  a:hover,
  a.active {
    color: var(--color-text-1);
  }

  a.active::after {
    position: absolute;
    right: 0;
    bottom: 0;
    left: 0;
    height: 3px;
    border-radius: 3px 3px 0 0;
    background: var(--color-primary);
    content: '';
  }

  .secondary-action {
    display: flex;
    width: max-content;
    height: 34px;
    align-items: center;
    gap: 0.35rem;
    padding: 0 0.7rem;
    margin-left: auto;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    background: var(--color-surface-2);
  }

  .secondary-action:hover {
    border-color: var(--color-primary);
    color: var(--color-primary);
  }

  .secondary-action::after {
    display: none;
  }

  .secondary-selector {
    position: relative;
    display: none;
    width: 100%;
    height: 100%;
    padding: 0 16px;
  }

  .secondary-selector > button {
    display: flex;
    width: 100%;
    height: 100%;
    align-items: center;
    justify-content: space-between;
    background: transparent;
    color: var(--color-text-1);
    font-size: 0.84rem;
    font-weight: 650;
  }

  .secondary-menu {
    position: absolute;
    z-index: 50;
    top: calc(100% + 8px);
    right: 16px;
    left: 16px;
    display: grid;
    padding: 0.45rem;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--color-surface-1);
    box-shadow: var(--shadow-float);
  }

  .secondary-menu a {
    display: flex;
    min-height: 42px;
    align-items: center;
    padding: 0 0.75rem;
    border-radius: var(--radius-sm);
  }

  .secondary-menu a.active {
    background: var(--color-primary-soft);
    color: var(--color-primary);
  }

  .secondary-menu a::after {
    display: none;
  }

  @media (max-width: 720px) {
    .secondary-inner {
      display: none;
    }

    .secondary-selector {
      display: flex;
      align-items: center;
      gap: 0.5rem;
    }

    .secondary-selector > button {
      min-width: 0;
      flex: 1;
      width: auto;
    }

    .secondary-selector > .secondary-action {
      flex: none;
      margin-left: 0;
    }
  }
</style>
