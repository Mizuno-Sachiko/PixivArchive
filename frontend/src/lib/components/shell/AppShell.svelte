<script lang="ts">
  import { page } from '$app/state';
  import type { Snippet } from 'svelte';

  import {
    navigationSectionFromPath,
    secondaryNavigationItems
  } from '$lib/navigation';
  import { detailReturnRoute } from '$lib/stores/detail-navigation';

  import CommandPalette from './CommandPalette.svelte';
  import SecondaryNav from './SecondaryNav.svelte';
  import TopBar from './TopBar.svelte';

  interface Props {
    children: Snippet;
    onLogout: () => Promise<void>;
  }

  let { children, onLogout }: Props = $props();
  let activeSection = $derived(navigationSectionFromPath(page.url.pathname));
  let secondaryItems = $derived(secondaryNavigationItems(activeSection));
  let galleryMain = $derived(page.url.pathname.startsWith('/gallery'));
  let overviewMain = $derived(page.url.pathname === '/overview');
  let secondaryAction = $derived.by(() => {
    if (!page.url.pathname.startsWith('/gallery/works/')) return undefined;
    return {
      label: '返回',
      href: detailReturnRoute(page.state)
    };
  });
</script>

<div class="app-shell">
  <TopBar {onLogout} />
  <SecondaryNav items={[...secondaryItems]} trailingAction={secondaryAction} />
  <main class:gallery-main={galleryMain} class:overview-main={overviewMain}>
    {@render children()}
  </main>
  <CommandPalette />
</div>

<style>
  .app-shell {
    min-height: 100vh;
    background: var(--color-bg);
  }

  main {
    --main-padding-top: 32px;
    --main-padding-inline: 24px;
    --main-padding-bottom: 64px;
    --main-viewport-height: calc(
      100dvh - var(--topbar-height) - var(--secondary-nav-height) -
        var(--main-padding-top) - var(--main-padding-bottom)
    );

    width: min(var(--content-width), 100%);
    padding: var(--main-padding-top) var(--main-padding-inline)
      var(--main-padding-bottom);
    margin: 0 auto;
  }

  main.gallery-main {
    --main-padding-bottom: 16px;

    width: min(1920px, 100%);
  }

  main.overview-main {
    --main-padding-bottom: 0px;
  }

  @media (max-width: 720px) {
    main {
      --main-padding-top: 24px;
      --main-padding-inline: 16px;
      --main-padding-bottom: 48px;
    }

    main.gallery-main {
      --main-padding-bottom: 16px;
    }

    main.overview-main {
      --main-padding-bottom: 0px;
    }
  }
</style>
