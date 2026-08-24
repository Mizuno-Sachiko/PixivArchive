<script lang="ts">
  import { ApiError } from '$lib/api/client';
  import { followingApi } from '$lib/api/following';
  import { LatestRequest } from '$lib/latest-request';
  import {
    isPixivAccountConflict,
    pixivAccountActionFailureMessage
  } from '$lib/pixiv-account-errors';
  import { pixivAccountStore } from '$lib/stores/pixiv-account.svelte';

  interface Props {
    pixivArtistId: number;
    artistName: string;
  }

  let { pixivArtistId, artistName }: Props = $props();
  let phase = $state<'loading' | 'ready' | 'error'>('loading');
  let followed = $state(false);
  let busy = $state(false);
  let error = $state('');
  let currentAccountId = $derived(
    pixivAccountStore.currentForAction?.account_id ?? null
  );
  const requests = new LatestRequest();

  $effect(() => {
    const artistId = pixivArtistId;
    const accountId = currentAccountId;
    const request = requests.begin();
    if (accountId) {
      void load(artistId, accountId, request);
    } else {
      phase = 'error';
      error = '请先配置Pixiv账户';
    }
    return () => {
      if (requests.isCurrent(request)) requests.invalidate();
    };
  });

  async function load(
    artistId: number,
    accountId: string,
    request = requests.begin()
  ): Promise<void> {
    phase = 'loading';
    error = '';
    try {
      const state = await followingApi.artistFollowState(artistId, accountId);
      if (!requests.isCurrent(request)) return;
      followed = state.followed;
      phase = 'ready';
    } catch (cause) {
      if (!requests.isCurrent(request)) return;
      error = readableError(cause, '关注状态暂时无法读取');
      phase = 'error';
    }
  }

  async function toggle(): Promise<void> {
    if (phase === 'error') {
      if (currentAccountId) await load(pixivArtistId, currentAccountId);
      return;
    }
    const accountId = currentAccountId;
    const artistId = pixivArtistId;
    if (phase !== 'ready' || busy || !accountId) return;
    busy = true;
    error = '';
    try {
      const state = await followingApi.updateArtistFollow(
        artistId,
        !followed,
        accountId
      );
      if (currentAccountId !== accountId || pixivArtistId !== artistId) return;
      followed = state.followed;
    } catch (cause) {
      error = pixivAccountActionFailureMessage(
        cause,
        readableError(cause, '关注状态暂时无法更新')
      );
      if (isPixivAccountConflict(cause)) void pixivAccountStore.load();
    } finally {
      busy = false;
    }
  }

  function readableError(cause: unknown, fallback: string): string {
    if (!(cause instanceof ApiError)) return fallback;
    if (cause.code === 'rate_limited') return 'Pixiv请求过于频繁，请稍后重试';
    if (cause.status === 404) return 'Pixiv作者或账户不可用';
    return fallback;
  }
</script>

<div class="artist-follow-action">
  <button
    class:primary-button={phase === 'ready' && !followed}
    class:secondary-button={phase !== 'ready' || followed}
    type="button"
    aria-pressed={phase === 'ready' ? followed : undefined}
    aria-label={phase === 'ready' && followed
      ? `取消关注${artistName}`
      : `关注${artistName}`}
    disabled={phase === 'loading' || busy || !currentAccountId}
    onclick={() => void toggle()}
  >
    {phase === 'loading'
      ? '读取中'
      : phase === 'error'
        ? '重新读取'
        : busy
          ? '处理中'
          : followed
            ? '已关注'
            : '关注'}
  </button>
  {#if error}<span class="follow-error" role="alert">{error}</span>{/if}
</div>

<style>
  .artist-follow-action {
    position: relative;
    display: inline-flex;
    align-items: center;
  }

  button {
    height: 36px;
    white-space: nowrap;
  }

  .follow-error {
    position: absolute;
    top: 50%;
    right: calc(100% + 0.45rem);
    width: max-content;
    max-width: 15rem;
    color: var(--color-error);
    font-size: 0.7rem;
    line-height: 1.35;
    text-align: right;
    transform: translateY(-50%);
  }

  @media (max-width: 720px) {
    .follow-error {
      max-width: min(15rem, calc(100vw - 11rem));
    }
  }
</style>
