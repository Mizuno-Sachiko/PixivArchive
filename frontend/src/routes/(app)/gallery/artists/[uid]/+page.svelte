<script lang="ts">
  import { page } from '$app/state';

  import { getArtist, type ArtistDetail } from '$lib/api/gallery';
  import ArtistFollowButton from '$lib/components/gallery/ArtistFollowButton.svelte';
  import GalleryWorkspace from '$lib/components/gallery/GalleryWorkspace.svelte';
  import RetryMessage from '$lib/components/ui/RetryMessage.svelte';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import { parseSourceId } from '$lib/gallery-routes';
  import { LatestRequest } from '$lib/latest-request';

  let artist = $state<ArtistDetail | null>(null);
  let error = $state('');
  let reload = $state(0);
  const requests = new LatestRequest();

  $effect(() => {
    const pixivArtistId = parseSourceId(page.params.uid);
    void reload;
    if (pixivArtistId === null) {
      artist = null;
      error = 'Pixiv UID无效';
      requests.invalidate();
      return;
    }
    const request = requests.begin();
    artist = null;
    error = '';
    void getArtist(pixivArtistId)
      .then((value) => {
        if (requests.isCurrent(request)) artist = value;
      })
      .catch(() => {
        if (requests.isCurrent(request)) error = '作者信息暂时无法读取';
      });
    return () => {
      if (requests.isCurrent(request)) requests.invalidate();
    };
  });
</script>

{#if artist}
  {@const currentArtist = artist}
  {#key currentArtist.id}
    {#snippet descriptionActions()}
      <ArtistFollowButton
        pixivArtistId={currentArtist.pixiv_artist_id}
        artistName={currentArtist.name}
      />
    {/snippet}
    <GalleryWorkspace
      title={currentArtist.name}
      description={`Pixiv UID：${currentArtist.pixiv_artist_id} · 本地${currentArtist.work_count}件作品`}
      externalUrl={`https://www.pixiv.net/users/${currentArtist.pixiv_artist_id}`}
      externalPlacement="description"
      {descriptionActions}
      baseGroups={[
        {
          mode: 'all',
          filters: [{ type: 'artist_id', value: currentArtist.id }]
        }
      ]}
    />
  {/key}
{:else if error}
  <RetryMessage message={error} onRetry={() => (reload += 1)} />
{:else}
  <EmptyState message="正在读取作者作品…" loading />
{/if}
