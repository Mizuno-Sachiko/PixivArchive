<script lang="ts">
  import { page } from '$app/state';

  import { getSeries, type SeriesDetail } from '$lib/api/gallery';
  import GalleryWorkspace from '$lib/components/gallery/GalleryWorkspace.svelte';
  import RetryMessage from '$lib/components/ui/RetryMessage.svelte';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import { parseSourceId } from '$lib/gallery-routes';
  import { LatestRequest } from '$lib/latest-request';

  let series = $state<SeriesDetail | null>(null);
  let error = $state('');
  let reload = $state(0);
  const requests = new LatestRequest();

  $effect(() => {
    const pixivSeriesId = parseSourceId(page.params.pixivId);
    void reload;
    if (pixivSeriesId === null) {
      series = null;
      error = 'Pixiv系列ID无效';
      requests.invalidate();
      return;
    }
    const request = requests.begin();
    series = null;
    error = '';
    void getSeries(pixivSeriesId)
      .then((value) => {
        if (requests.isCurrent(request)) series = value;
      })
      .catch(() => {
        if (requests.isCurrent(request)) error = '系列信息暂时无法读取';
      });
    return () => {
      if (requests.isCurrent(request)) requests.invalidate();
    };
  });
</script>

{#if series}
  {#key series.id}
    <GalleryWorkspace
      title={series.title}
      description={`Pixiv系列 ${series.pixiv_series_id} · 本地${series.work_count}件作品`}
      externalUrl={series.pixiv_artist_id
        ? `https://www.pixiv.net/user/${series.pixiv_artist_id}/series/${series.pixiv_series_id}`
        : undefined}
      externalPlacement="description"
      baseGroups={[
        {
          mode: 'all',
          filters: [{ type: 'series_id', value: series.id }]
        }
      ]}
    />
  {/key}
{:else if error}
  <RetryMessage message={error} onRetry={() => (reload += 1)} />
{:else}
  <EmptyState message="正在读取系列作品…" loading />
{/if}
