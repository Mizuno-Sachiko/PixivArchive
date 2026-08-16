<script lang="ts">
  import { resolve } from '$app/paths';
  import { page } from '$app/state';

  import { commandPaletteStore } from '$lib/stores/command-palette.svelte';
  import { pixivAccountStore } from '$lib/stores/pixiv-account.svelte';
  import { themeStore, type ThemeMode } from '$lib/stores/theme.svelte';
  import ClearPixivCredentialAction from '$lib/components/account/ClearPixivCredentialAction.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import AccountAvatar from '$lib/components/ui/AccountAvatar.svelte';
  import {
    navigationSectionFromPath,
    primaryNavigationItems
  } from '$lib/navigation';
  import { isPixivAccountAvailable } from '$lib/pixiv-account-status';
  import Brand from './Brand.svelte';
  import TopBarPopover from './TopBarPopover.svelte';

  interface Props {
    onLogout: () => Promise<void>;
  }

  const themeItems: Array<{ label: string; value: ThemeMode }> = [
    { label: '跟随系统', value: 'system' },
    { label: '浅色', value: 'light' },
    { label: '深色', value: 'dark' }
  ];

  let { onLogout }: Props = $props();
  let themeOpen = $state(false);
  let accountOpen = $state(false);
  let mobileOpen = $state(false);
  let logoutBusy = $state(false);
  let logoutError = $state('');
  let activeSection = $derived(navigationSectionFromPath(page.url.pathname));
  let activePixivAccount = $derived(
    pixivAccountStore.current &&
      isPixivAccountAvailable(pixivAccountStore.current.state)
      ? pixivAccountStore.current
      : null
  );
  let avatarUrl = $derived(activePixivAccount?.avatar_url ?? '');

  function selectTheme(mode: ThemeMode): void {
    themeStore.setMode(mode);
    themeOpen = false;
  }

  function toggleTheme(): void {
    themeOpen = !themeOpen;
    accountOpen = false;
    mobileOpen = false;
  }

  function toggleAccount(): void {
    accountOpen = !accountOpen;
    themeOpen = false;
    mobileOpen = false;
  }

  function toggleNavigation(): void {
    mobileOpen = !mobileOpen;
    themeOpen = false;
    accountOpen = false;
  }

  async function logout(): Promise<void> {
    if (logoutBusy) return;
    logoutBusy = true;
    logoutError = '';
    try {
      await onLogout();
    } catch {
      logoutError = '退出登录失败，请稍后重试';
    } finally {
      logoutBusy = false;
    }
  }
</script>

<header class="topbar glass-surface">
  <div class="topbar-inner">
    <Brand />

    <nav aria-label="主要导航" class="primary-nav">
      {#each primaryNavigationItems as item (item.href)}
        <a
          class:active={activeSection === item.section}
          href={resolve(item.href)}>{item.label}</a
        >
      {/each}
    </nav>

    <button
      class="search-trigger"
      type="button"
      aria-label="全局搜索"
      onclick={() => commandPaletteStore.open()}
    >
      <Icon name="search" size={18} />
      <span>搜索作品、作者、标签、系列和页面</span>
      <kbd><Icon name="command" size={13} />K</kbd>
    </button>

    <div class="actions">
      <div class="popover-anchor">
        <button
          class="icon-button"
          type="button"
          aria-label="主题"
          aria-expanded={themeOpen}
          onclick={toggleTheme}
        >
          <Icon name="theme" />
        </button>
        {#if themeOpen}
          <TopBarPopover kind="theme">
            <p class="popover-title">主题</p>
            <div class="theme-options">
              {#each themeItems as item (item.value)}
                <button
                  class="popover-item"
                  type="button"
                  class:active={themeStore.mode === item.value}
                  onclick={() => selectTheme(item.value)}
                >
                  <span>{item.label}</span>
                  {#if themeStore.mode === item.value}<Icon
                      name="check"
                      size={16}
                    />{/if}
                </button>
              {/each}
            </div>
          </TopBarPopover>
        {/if}
      </div>

      <div class="popover-anchor">
        <button
          class="avatar-button"
          type="button"
          aria-label="管理员菜单"
          aria-expanded={accountOpen}
          onclick={toggleAccount}
        >
          <AccountAvatar src={avatarUrl} size={38} />
        </button>
        {#if accountOpen}
          <TopBarPopover kind="account">
            <div class="account-name">
              <strong>{activePixivAccount?.display_name ?? '管理员'}</strong>
              <span>
                {#if activePixivAccount?.pixiv_user_id}
                  Pixiv ID {activePixivAccount.pixiv_user_id}
                {:else}
                  本地账户
                {/if}
              </span>
            </div>
            <a class="popover-item" href={resolve('/system/account')}
              >账户与安全</a
            >
            <ClearPixivCredentialAction variant="menu" />
            {#if logoutError}<span class="logout-error" role="alert"
                >{logoutError}</span
              >{/if}
            <button
              class="popover-item"
              type="button"
              disabled={logoutBusy}
              onclick={logout}>{logoutBusy ? '正在退出…' : '退出登录'}</button
            >
          </TopBarPopover>
        {/if}
      </div>

      <button
        class="icon-button mobile-toggle"
        type="button"
        aria-label="打开导航"
        aria-expanded={mobileOpen}
        onclick={toggleNavigation}
      >
        <Icon name={mobileOpen ? 'close' : 'menu'} />
      </button>
    </div>
  </div>

  {#if mobileOpen}
    <TopBarPopover kind="navigation">
      <nav class="mobile-nav" aria-label="移动导航">
        {#each primaryNavigationItems as item (item.href)}
          <a
            class:active={activeSection === item.section}
            href={resolve(item.href)}
            onclick={() => (mobileOpen = false)}
          >
            {item.label}
          </a>
        {/each}
        <button
          type="button"
          onclick={() => {
            mobileOpen = false;
            commandPaletteStore.open();
          }}
        >
          <Icon name="search" size={18} />
          全局搜索
        </button>
      </nav>
    </TopBarPopover>
  {/if}
</header>

<style>
  .topbar {
    position: sticky;
    z-index: 40;
    top: 0;
    height: var(--topbar-height);
    border-width: 0 0 1px;
    border-radius: 0;
    box-shadow: none;
  }

  .topbar-inner {
    display: grid;
    width: min(var(--content-width), 100%);
    height: 100%;
    grid-template-columns: auto auto minmax(210px, 1fr) auto;
    gap: clamp(1rem, 2vw, 2.25rem);
    align-items: center;
    padding: 0 24px;
    margin: 0 auto;
  }

  .primary-nav {
    display: flex;
    height: 100%;
    align-items: center;
    gap: 0.25rem;
  }

  .primary-nav a {
    position: relative;
    display: grid;
    height: 100%;
    padding: 0 0.72rem;
    place-items: center;
    color: var(--color-text-2);
    font-size: 0.88rem;
    font-weight: 650;
  }

  .primary-nav a:hover,
  .primary-nav a.active {
    color: var(--color-text-1);
  }

  .primary-nav a.active::after {
    position: absolute;
    right: 0.7rem;
    bottom: 0;
    left: 0.7rem;
    height: 3px;
    border-radius: 3px 3px 0 0;
    background: var(--color-primary);
    content: '';
  }

  .search-trigger {
    display: grid;
    width: min(100%, 470px);
    height: 40px;
    grid-template-columns: auto 1fr auto;
    gap: 0.65rem;
    align-items: center;
    justify-self: center;
    padding: 0 0.72rem;
    border-radius: var(--radius-pill);
    background: var(--color-surface-2);
    color: var(--color-text-3);
    text-align: left;
  }

  .search-trigger:hover {
    background: var(--color-surface-3);
    color: var(--color-text-2);
  }

  .search-trigger span {
    overflow: hidden;
    font-size: 0.84rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  kbd {
    display: inline-flex;
    align-items: center;
    gap: 0.1rem;
    padding: 0.18rem 0.38rem;
    border: 1px solid var(--color-border);
    border-radius: 6px;
    background: var(--color-surface-1);
    color: var(--color-text-3);
    font: 0.7rem var(--font-ui);
  }

  .actions {
    display: flex;
    align-items: center;
    gap: 0.45rem;
  }

  .popover-anchor {
    position: relative;
  }

  .icon-button,
  .avatar-button {
    display: grid;
    width: 38px;
    height: 38px;
    place-items: center;
    border-radius: 50%;
    background: transparent;
    color: var(--color-text-2);
  }

  .icon-button:hover,
  .avatar-button:hover,
  .icon-button[aria-expanded='true'],
  .avatar-button[aria-expanded='true'] {
    background: var(--color-surface-3);
    color: var(--color-text-1);
  }

  .avatar-button {
    min-width: 38px;
    flex: 0 0 38px;
    overflow: hidden;
    padding: 0;
    background: transparent;
  }

  .popover-title {
    padding: 0.45rem 0.55rem 0.35rem;
    margin: 0;
    color: var(--color-text-3);
    font-size: 0.72rem;
    font-weight: 700;
  }

  .popover-item {
    display: flex;
    width: 100%;
    min-height: 38px;
    align-items: center;
    justify-content: space-between;
    padding: 0.45rem 0.55rem;
    border-radius: 8px;
    background: transparent;
    font-size: 0.84rem;
    text-align: left;
  }

  .popover-item:hover,
  .popover-item.active {
    background: var(--color-primary-soft);
  }

  .theme-options {
    display: grid;
    gap: 0.25rem;
  }

  .account-name {
    padding: 0.55rem;
    border-bottom: 1px solid var(--color-border);
    margin-bottom: 0.35rem;
  }

  .account-name strong,
  .account-name span {
    display: block;
  }

  .logout-error {
    display: block;
    padding: 0.45rem 0.55rem;
    color: var(--color-error);
    font-size: 0.72rem;
    line-height: 1.45;
  }

  .account-name span {
    margin-top: 0.15rem;
    color: var(--color-text-3);
    font-size: 0.72rem;
  }

  .mobile-toggle {
    display: none;
  }

  .mobile-nav {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 0.4rem;
  }

  .mobile-nav a,
  .mobile-nav button {
    display: flex;
    min-height: 44px;
    align-items: center;
    justify-content: center;
    gap: 0.45rem;
    border-radius: 10px;
    background: transparent;
    color: var(--color-text-2);
    font-size: 0.86rem;
    font-weight: 640;
  }

  .mobile-nav a.active,
  .mobile-nav a:hover,
  .mobile-nav button:hover {
    background: var(--color-primary-soft);
    color: var(--color-primary);
  }

  @media (max-width: 1080px) {
    .topbar-inner {
      grid-template-columns: auto auto 1fr auto;
      gap: 0.8rem;
    }

    .search-trigger {
      width: 42px;
      grid-template-columns: 1fr;
      place-items: center;
      padding: 0;
    }

    .search-trigger span,
    .search-trigger kbd {
      display: none;
    }
  }

  @media (max-width: 800px) {
    .topbar-inner {
      grid-template-columns: auto 1fr auto;
      padding: 0 16px;
    }

    .primary-nav {
      display: none;
    }

    .search-trigger {
      justify-self: end;
    }

    .mobile-toggle {
      display: grid;
    }
  }
</style>
