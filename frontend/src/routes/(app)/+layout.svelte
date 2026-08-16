<script lang="ts">
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { onMount, type Snippet } from 'svelte';

  import AppShell from '$lib/components/shell/AppShell.svelte';
  import Button from '$lib/components/ui/Button.svelte';
  import { LatestRequest } from '$lib/latest-request';
  import { appEventsStore } from '$lib/stores/app-events.svelte';
  import { contentSettingsStore } from '$lib/stores/content-settings.svelte';
  import { csrfStore } from '$lib/stores/csrf.svelte';
  import { overviewDecorationsStore } from '$lib/stores/overview-decorations.svelte';
  import { pixivAccountStore } from '$lib/stores/pixiv-account.svelte';
  import { sessionStore } from '$lib/stores/session.svelte';

  interface Props {
    children: Snippet;
  }

  let { children }: Props = $props();
  let ready = $state(false);
  let loadError = $state('');
  let refreshEventsActive = $state(false);
  let observedAccountRevision = $state(-1);
  let observedContentRevision = $state(-1);
  let observedAccountSnapshotRevision = $state(-1);
  let observedContentSnapshotRevision = $state(-1);
  let disposed = false;
  const restoreRequests = new LatestRequest();

  $effect(() => {
    const accountRevision = appEventsStore.resourceRevisions.pixiv_account;
    const snapshotRevision = appEventsStore.snapshotRevision;
    if (
      !refreshEventsActive ||
      (accountRevision === observedAccountRevision &&
        snapshotRevision === observedAccountSnapshotRevision)
    ) {
      return;
    }
    observedAccountRevision = accountRevision;
    observedAccountSnapshotRevision = snapshotRevision;
    void pixivAccountStore.load();
  });

  $effect(() => {
    const contentRevision = appEventsStore.resourceRevisions.system_setting;
    const snapshotRevision = appEventsStore.snapshotRevision;
    if (
      !refreshEventsActive ||
      (contentRevision === observedContentRevision &&
        snapshotRevision === observedContentSnapshotRevision)
    ) {
      return;
    }
    observedContentRevision = contentRevision;
    observedContentSnapshotRevision = snapshotRevision;
    void contentSettingsStore.load();
  });

  onMount(() => {
    disposed = false;
    void restoreSession();

    return () => {
      disposed = true;
      refreshEventsActive = false;
      restoreRequests.invalidate();
      appEventsStore.disconnect();
      pixivAccountStore.reset();
      contentSettingsStore.reset();
      overviewDecorationsStore.reset();
    };
  });

  async function restoreSession(): Promise<void> {
    const request = restoreRequests.begin();
    loadError = '';
    try {
      const session = await sessionStore.restore();
      if (disposed || !restoreRequests.isCurrent(request)) return;
      if (!session) {
        await goto(resolve('/login'), { replaceState: true });
        return;
      }
      csrfStore.refresh();
      observedAccountRevision = appEventsStore.resourceRevisions.pixiv_account;
      observedContentRevision = appEventsStore.resourceRevisions.system_setting;
      observedAccountSnapshotRevision = appEventsStore.snapshotRevision;
      observedContentSnapshotRevision = appEventsStore.snapshotRevision;
      await Promise.all([
        pixivAccountStore.load(),
        contentSettingsStore.load()
      ]);
      if (disposed || !restoreRequests.isCurrent(request)) return;
      appEventsStore.connect();
      refreshEventsActive = true;
      ready = true;
    } catch {
      if (!disposed && restoreRequests.isCurrent(request)) {
        loadError = '无法读取当前会话，请检查Web服务状态。';
      }
    }
  }

  async function logout(): Promise<void> {
    await sessionStore.signOut();
    refreshEventsActive = false;
    ready = false;
    csrfStore.clear();
    appEventsStore.disconnect();
    pixivAccountStore.reset();
    contentSettingsStore.reset();
    overviewDecorationsStore.reset();
    await goto(resolve('/login'));
  }
</script>

{#if ready}
  <AppShell onLogout={logout}>{@render children()}</AppShell>
{:else if loadError}
  <main class="session-state" role="alert">
    <strong>无法进入管理界面</strong>
    <p>{loadError}</p>
    <Button class="session-retry" onclick={() => void restoreSession()}
      >重新验证</Button
    >
  </main>
{:else}
  <main class="session-state" aria-label="正在验证会话">
    <span class="loading-line"></span>
  </main>
{/if}

<style>
  .session-state {
    display: grid;
    min-height: 100vh;
    place-content: center;
    background: var(--color-bg);
    color: var(--color-text-2);
    text-align: center;
  }

  .session-state p {
    margin: 0.5rem 0 0;
    font-size: 0.86rem;
  }

  :global(.session-retry) {
    justify-self: center;
    margin-top: 0.9rem;
  }

  .loading-line {
    width: 36px;
    height: 3px;
    overflow: hidden;
    border-radius: var(--radius-pill);
    background: var(--color-surface-3);
  }

  .loading-line::after {
    display: block;
    width: 45%;
    height: 100%;
    background: var(--color-primary);
    content: '';
    animation: progress 750ms var(--ease-standard) infinite alternate;
  }

  @keyframes progress {
    to {
      transform: translateX(122%);
    }
  }
</style>
