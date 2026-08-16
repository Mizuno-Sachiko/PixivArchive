<script lang="ts">
  import GalleryWorkspace from '$lib/components/gallery/GalleryWorkspace.svelte';
  import AlertBanner from '$lib/components/ui/AlertBanner.svelte';
  import PageHeader from '$lib/components/ui/PageHeader.svelte';
  import {
    isPixivAccountAvailable,
    pixivAccountFavoritesNotice
  } from '$lib/pixiv-account-status';
  import { pixivAccountStore } from '$lib/stores/pixiv-account.svelte';

  let account = $derived(pixivAccountStore.current);
  let accountAvailable = $derived(
    Boolean(account && isPixivAccountAvailable(account.state))
  );
  let accountNotice = $derived(
    account ? pixivAccountFavoritesNotice(account.state) : null
  );
</script>

{#if accountAvailable}
  <GalleryWorkspace
    title="收藏"
    baseGroups={[
      {
        mode: 'all',
        filters: [
          {
            type: 'boolean',
            field: 'bookmarked_by_current_account',
            value: true
          }
        ]
      }
    ]}
  />
{:else}
  <section class="workspace-page">
    <PageHeader title="收藏" />
    {#if accountNotice}
      <AlertBanner
        title={accountNotice.title}
        message={accountNotice.message}
        tone={accountNotice.tone}
      />
    {:else if pixivAccountStore.error}
      <AlertBanner
        title="Pixiv账户资料暂时无法读取"
        message="重新读取账户资料后，才能显示对应账户的收藏。"
        tone="error"
      />
    {/if}
  </section>
{/if}
